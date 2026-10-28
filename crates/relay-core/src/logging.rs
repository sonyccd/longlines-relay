//! Diagnostics: a daily rolling log file plus stderr.
//!
//! Nothing logs the token: [`crate::upstream::token_store::Token`] and the device code print as
//! `<redacted>`, and the client never logs headers or bodies.

use std::path::Path;

/// Keep the value [`init`] returns alive for the life of the process.
pub use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt, EnvFilter};

/// Log files are `relay.<date>.log`.
pub const LOG_FILE_PREFIX: &str = "relay";
/// Daily files kept.
pub const LOG_FILES_KEPT: usize = 14;

#[derive(Debug, thiserror::Error)]
pub enum LoggingError {
    #[error("couldn't open the log folder: {0}")]
    Appender(#[from] tracing_appender::rolling::InitError),
    #[error("couldn't create the log folder: {0}")]
    Io(#[from] std::io::Error),
    #[error("logging is already set up: {0}")]
    Init(#[from] tracing_subscriber::util::TryInitError),
}

/// A daily rolling appender in `dir`, keeping [`LOG_FILES_KEPT`] files.
pub fn file_appender(dir: &Path) -> Result<RollingFileAppender, LoggingError> {
    std::fs::create_dir_all(dir)?;
    Ok(RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix(LOG_FILE_PREFIX)
        .filename_suffix("log")
        .max_log_files(LOG_FILES_KEPT)
        .build(dir)?)
}

/// Sends `tracing` output to the log folder and stderr. Keep the guard alive for the life of the
/// process; dropping it flushes the file. `RUST_LOG` overrides the default `info` level.
pub fn init(dir: &Path) -> Result<WorkerGuard, LoggingError> {
    let (writer, guard) = tracing_appender::non_blocking(file_appender(dir)?);
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_ansi(false).with_writer(writer))
        .with(fmt::layer().with_writer(std::io::stderr))
        .try_init()?;
    Ok(guard)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn appender_writes_a_dated_file() {
        let dir = tempfile::tempdir().unwrap();
        let logs = dir.path().join("logs");
        let mut appender = file_appender(&logs).unwrap();
        appender.write_all(b"hello\n").unwrap();
        appender.flush().unwrap();
        let names: Vec<String> = std::fs::read_dir(&logs)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names.len(), 1);
        assert!(
            names[0].starts_with("relay.") && names[0].ends_with(".log"),
            "{names:?}"
        );
    }
}
