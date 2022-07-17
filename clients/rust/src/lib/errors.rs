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

// We'll put our errors in an `errors` module, and other modules in this crate will
// `use crate::errors::*;` to get access to everything `error_chain!` creates.
//#[doc(hidden)]
//pub mod errors {
// Create the Error, ErrorKind, ResultExt, and Result types
//    error_chain! {}
//}
