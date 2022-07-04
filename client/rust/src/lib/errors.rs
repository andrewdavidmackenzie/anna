#![allow(missing_docs)]

pub use error_chain::bail;
use error_chain::error_chain;

error_chain! {
    types {
        Error, ErrorKind, ResultExt, Result;
    }

    foreign_links {
        Io(std::io::Error);
        Serde(serde_yaml::Error);
    }
}
