use thiserror::Error;

pub mod consts;
pub mod version;

pub type Result<T, E = Error> = std::result::Result<T, E>;


#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}: No such file or directory.")]
    FileNotFound(String),
    #[error("{0}")]
    Other(String),
}


#[macro_export]
macro_rules! print_flush {
    ($($arg:tt)*) => {{
        use std::io::{Write, stdout};
        print!($($arg)*);
        stdout().flush().unwrap();
    }};
}
