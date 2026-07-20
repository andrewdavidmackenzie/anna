/// Errors returned by the anna client library.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// I/O error (file, network, etc.)
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// A KVS operation failed
    #[error("KVS error: {0}")]
    Kvs(String),

    /// Process management error
    #[error("Process error: {0}")]
    Process(String),
}

/// Convenience alias for `std::result::Result<T, Error>`.
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn io_error_display() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let err: Error = io_err.into();
        assert!(err.to_string().contains("I/O error"));
    }

    #[test]
    fn kvs_error_display() {
        let err = Error::Kvs("timeout".into());
        assert!(err.to_string().contains("KVS error: timeout"));
    }

    #[test]
    fn process_error_display() {
        let err = Error::Process("spawn failed".into());
        assert!(err.to_string().contains("Process error: spawn failed"));
    }
}
