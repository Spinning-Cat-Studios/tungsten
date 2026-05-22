//! zstd compression wrappers for full-output cache entries (ADR 10.5.26o).
//!
//! Provides compress/decompress functions using zstd level 1 (speed-optimized).
//! Only compiled when the `compress` feature is enabled.

/// Compress bytes using zstd level 1.
///
/// Returns the compressed payload. Level 1 gives ~3–5× compression ratio on
/// structured binary data at ~500MB/s throughput.
#[cfg(feature = "compress")]
pub fn compress(data: &[u8]) -> std::io::Result<Vec<u8>> {
    zstd::encode_all(std::io::Cursor::new(data), 1)
}

/// Decompress zstd-compressed bytes.
///
/// Returns the decompressed payload, or an error if the data is not valid zstd.
#[cfg(feature = "compress")]
pub fn decompress(data: &[u8]) -> std::io::Result<Vec<u8>> {
    zstd::decode_all(std::io::Cursor::new(data))
}

#[cfg(test)]
#[cfg(feature = "compress")]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_empty() {
        let compressed = compress(b"").unwrap();
        let decompressed = decompress(&compressed).unwrap();
        assert_eq!(decompressed, b"");
    }

    #[test]
    fn roundtrip_small_payload() {
        let data = b"hello world repeated many times for compression benefit".repeat(100);
        let compressed = compress(&data).unwrap();
        let decompressed = decompress(&compressed).unwrap();
        assert_eq!(decompressed, data);
        // Compression should actually shrink this repetitive data
        assert!(compressed.len() < data.len());
    }

    #[test]
    fn roundtrip_random_like_data() {
        // Less compressible data (sequential bytes)
        let data: Vec<u8> = (0..10_000).map(|i| (i % 256) as u8).collect();
        let compressed = compress(&data).unwrap();
        let decompressed = decompress(&compressed).unwrap();
        assert_eq!(decompressed, data);
    }

    #[test]
    fn decompress_invalid_data_returns_error() {
        let result = decompress(b"this is not valid zstd data");
        assert!(result.is_err());
    }

    #[test]
    fn compression_ratio_on_structured_data() {
        // Simulate bincode-like structured data: many small integers and strings
        let mut data = Vec::new();
        for i in 0u32..1000 {
            data.extend_from_slice(&i.to_le_bytes());
            data.extend_from_slice(b"some_type_name_that_repeats_often");
            data.extend_from_slice(&[0u8; 8]); // padding/spans
        }
        let compressed = compress(&data).unwrap();
        let ratio = data.len() as f64 / compressed.len() as f64;
        // Expect at least 2.5× compression on this kind of structured data
        assert!(
            ratio >= 2.5,
            "compression ratio {ratio:.1}× is below minimum 2.5×"
        );
    }

    #[test]
    fn roundtrip_large_payload() {
        // 10 MB payload to validate no streaming/buffering issues
        let mut data = Vec::with_capacity(10 * 1024 * 1024);
        for i in 0u32..(10 * 1024 * 1024 / 4) {
            data.extend_from_slice(&i.to_le_bytes());
        }
        assert_eq!(data.len(), 10 * 1024 * 1024);
        let compressed = compress(&data).unwrap();
        let decompressed = decompress(&compressed).unwrap();
        assert_eq!(decompressed, data);
    }
}
