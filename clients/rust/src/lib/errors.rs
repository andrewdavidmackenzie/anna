/// Errors returned by the anna client library.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// I/O error (file, network, etc.)
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// YAML config parsing error
    #[error("Config parse error: {0}")]
    ConfigParse(#[from] serde_yaml::Error),

    /// Config file could not be loaded
    #[error("Could not load config from '{path}': {detail}")]
    ConfigFile {
        /// Path to the config file
        path: String,
        /// What went wrong
        detail: String,
    },

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
    fn config_file_error_display() {
        let err = Error::ConfigFile {
            path: "/etc/anna.yml".into(),
            detail: "not found".into(),
        };
        assert!(err.to_string().contains("/etc/anna.yml"));
        assert!(err.to_string().contains("not found"));
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
