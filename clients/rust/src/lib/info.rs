//! Info module provides methods to get additional information about the `anna` library

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Return the version number of the `annalib` library.
///
/// ```rust
/// let version = annalib::info::version();
/// assert!(!version.is_empty());
/// ```
pub fn version() -> &'static str {
    VERSION
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn can_get_version() {
        assert!(!version().is_empty());
    }
}
