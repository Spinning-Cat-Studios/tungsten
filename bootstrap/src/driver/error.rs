//! Pipeline error types.

/// Pipeline errors.
#[derive(Debug)]
pub enum PipelineError {
    /// Failed to read a file
    IoError(String, String),
    /// Elaboration or type checking failed
    ElabFailed(String),
    /// Circular module dependency detected
    ModuleCycle {
        /// Path where the cycle was detected
        path: std::path::PathBuf,
        /// The cycle chain (for error message)
        chain: Vec<std::path::PathBuf>,
    },
    /// Both file.tg and file/mod.tg exist
    AmbiguousModule {
        name: String,
        file: std::path::PathBuf,
        dir: std::path::PathBuf,
    },
    /// Module file not found
    ModuleNotFound {
        name: String,
        searched: Vec<std::path::PathBuf>,
        referenced_from: std::path::PathBuf,
    },
}

impl std::fmt::Display for PipelineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PipelineError::IoError(path, msg) => write!(f, "could not read '{}': {}", path, msg),
            PipelineError::ElabFailed(msg) => write!(f, "compilation failed: {}", msg),
            PipelineError::ModuleCycle { path, chain } => {
                write!(
                    f,
                    "circular module dependency detected at '{}'\n",
                    path.display()
                )?;
                write!(f, "  cycle: ")?;
                for (i, p) in chain.iter().enumerate() {
                    if i > 0 {
                        write!(f, " -> ")?;
                    }
                    write!(f, "{}", p.display())?;
                }
                Ok(())
            }
            PipelineError::AmbiguousModule { name, file, dir } => {
                write!(
                    f,
                    "ambiguous module '{}': both '{}' and '{}' exist",
                    name,
                    file.display(),
                    dir.display()
                )
            }
            PipelineError::ModuleNotFound {
                name,
                searched,
                referenced_from,
            } => {
                write!(
                    f,
                    "module '{}' not found (referenced from '{}')\n  searched: {}",
                    name,
                    referenced_from.display(),
                    searched
                        .iter()
                        .map(|p| p.display().to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
        }
    }
}
