use std::env;
use std::path::{Path, PathBuf};

use crate::common::{Error, Result};

pub fn get_absolute_path<P: AsRef<Path>>(path: P) -> Result<PathBuf> {
    let path = path.as_ref();
    let path_string = path.to_string_lossy().to_string();
    if path.is_absolute() {
        if path.exists() {
            return Ok(path.to_path_buf());
        }
        return Err(Error::FileNotFound(path_string));
    }
    let current_dir = env::current_dir();
    match current_dir {
        Ok(current_dir) => {
            let path = current_dir.join(path);
            if path.exists() {
                return Ok(path);
            }
        }
        Err(e) => {
            return Err(Error::Other(e.to_string()));
        }
    }
    Err(Error::FileNotFound(path_string))
}

pub fn get_absolute_paths<P: AsRef<Path>>(paths: Vec<P>) -> Result<(Vec<PathBuf>, Vec<PathBuf>)> {
    let mut result = vec![];
    let mut not_found = vec![];
    for path in paths {
        let p = get_absolute_path(path.as_ref());
        match p {
            Ok(p) => {
                result.push(p);
            }
            Err(e) => {
                match e {
                    Error::FileNotFound(_) => {
                        not_found.push(path.as_ref().to_path_buf());
                    }
                    _ => {
                        return Err(e);
                    }
                }
            }
        }
    }
    Ok((result, not_found))
}