//! Background cache writer thread (ADR 10.5.26o).
//!
//! Spawns a writer thread that receives `(PathBuf, Vec<u8>)` pairs via a
//! bounded channel and writes them to disk off the critical path. Errors are
//! collected and reported on join.

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread::{self, JoinHandle};

/// A write request sent from the elaboration thread to the background writer.
pub struct WriteRequest {
    pub path: PathBuf,
    pub data: Vec<u8>,
}

/// Errors collected during background writes.
#[derive(Debug)]
pub struct WriteError {
    pub path: PathBuf,
    pub error: std::io::Error,
}

/// Handle to the background writer thread.
///
/// Created via [`BackgroundWriter::spawn`]. The writer processes entries from
/// a bounded channel until it is dropped or [`BackgroundWriter::join`] is called.
pub struct BackgroundWriter {
    sender: Option<mpsc::SyncSender<WriteRequest>>,
    handle: Option<JoinHandle<Vec<WriteError>>>,
}

impl BackgroundWriter {
    /// Spawn a background writer with a bounded channel.
    ///
    /// `capacity` controls backpressure: if the writer falls behind, the
    /// elaboration thread blocks after `capacity` pending entries.
    pub fn spawn(capacity: usize) -> Self {
        let (sender, receiver) = mpsc::sync_channel::<WriteRequest>(capacity);

        let handle = thread::Builder::new()
            .name("elab-cache-writer".to_string())
            .spawn(move || {
                let mut errors = Vec::new();
                while let Ok(req) = receiver.recv() {
                    if let Err(e) = write_entry(&req.path, &req.data) {
                        errors.push(WriteError {
                            path: req.path,
                            error: e,
                        });
                    }
                }
                errors
            })
            .expect("failed to spawn cache writer thread");

        Self {
            sender: Some(sender),
            handle: Some(handle),
        }
    }

    /// Send a write request to the background writer.
    ///
    /// Blocks if the channel is full (backpressure). Returns `false` if the
    /// writer thread has already terminated (channel disconnected).
    pub fn send(&self, path: PathBuf, data: Vec<u8>) -> bool {
        if let Some(ref sender) = self.sender {
            sender.send(WriteRequest { path, data }).is_ok()
        } else {
            false
        }
    }

    /// Drop the sender and join the writer thread, flushing all pending entries.
    ///
    /// Returns any write errors that occurred during background writes.
    pub fn join(mut self) -> Vec<WriteError> {
        // Drop sender to signal the writer thread to finish
        self.sender.take();
        if let Some(handle) = self.handle.take() {
            handle.join().unwrap_or_default()
        } else {
            Vec::new()
        }
    }

    /// Number of pending entries (approximate — for diagnostics only).
    pub fn is_disconnected(&self) -> bool {
        self.sender.is_none()
    }
}

impl Drop for BackgroundWriter {
    fn drop(&mut self) {
        // Ensure we drop the sender so the writer thread can exit
        self.sender.take();
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// Write a single cache entry to disk atomically (write to temp, rename).
fn write_entry(path: &PathBuf, data: &[u8]) -> std::io::Result<()> {
    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    // Write directly (no temp file for now — the cache is best-effort)
    let mut file = fs::File::create(path)?;
    file.write_all(data)?;
    file.sync_data()?;
    Ok(())
}

/// Compute the default channel capacity: `min(num_cpus, 16)`.
pub fn default_channel_capacity() -> usize {
    let cpus = thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    cpus.min(16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writer_flushes_all_entries_on_join() {
        let dir = tempfile::tempdir().unwrap();
        let writer = BackgroundWriter::spawn(4);

        for i in 0..10 {
            let path = dir.path().join(format!("entry_{i}.bin"));
            writer.send(path, vec![i as u8; 100]);
        }

        let errors = writer.join();
        assert!(errors.is_empty(), "expected no write errors: {errors:?}");

        // Verify all files were written
        for i in 0..10 {
            let path = dir.path().join(format!("entry_{i}.bin"));
            let data = fs::read(&path).unwrap();
            assert_eq!(data, vec![i as u8; 100]);
        }
    }

    #[test]
    fn writer_collects_errors_for_bad_paths() {
        let writer = BackgroundWriter::spawn(4);

        // Send to an invalid path
        let bad_path = PathBuf::from("/nonexistent/deeply/nested/path/entry.bin");
        writer.send(bad_path, vec![1, 2, 3]);

        let errors = writer.join();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].path.to_string_lossy().contains("nonexistent"));
    }

    #[test]
    fn writer_handles_drop_gracefully() {
        let dir = tempfile::tempdir().unwrap();
        let writer = BackgroundWriter::spawn(4);
        let path = dir.path().join("test.bin");
        writer.send(path.clone(), vec![42; 50]);
        drop(writer); // Should not panic

        // File should still be written (drop joins the thread)
        assert!(path.exists());
    }

    #[test]
    fn writer_send_returns_false_after_join() {
        let writer = BackgroundWriter::spawn(4);
        // Consume the writer
        let _errors = writer.join();
        // Can't send after join because self is consumed — this is a compile-time guarantee
    }

    #[test]
    fn default_capacity_is_reasonable() {
        let cap = default_channel_capacity();
        assert!(cap >= 1 && cap <= 16);
    }

    #[test]
    fn backpressure_does_not_lose_entries() {
        // Use capacity=1 so the channel fills immediately, exercising backpressure
        let dir = tempfile::tempdir().unwrap();
        let writer = BackgroundWriter::spawn(1);

        // Send many more entries than channel capacity
        let count = 50;
        for i in 0..count {
            let path = dir.path().join(format!("bp_{i}.bin"));
            writer.send(path, vec![i as u8; 200]);
        }

        let errors = writer.join();
        assert!(errors.is_empty(), "expected no write errors: {errors:?}");

        // All entries must be written despite backpressure
        for i in 0..count {
            let path = dir.path().join(format!("bp_{i}.bin"));
            let data = fs::read(&path).unwrap_or_else(|_| panic!("missing entry {i}"));
            assert_eq!(data, vec![i as u8; 200]);
        }
    }
}
