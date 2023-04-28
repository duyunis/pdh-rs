use std::env;
use std::path::{Path, PathBuf};

use crate::common::{Error, Result};

#[derive(Debug, Clone)]
pub struct FileInfo {
    pub name: String,
    pub absolute_path: String,
    pub size: u64,
    pub is_dir: bool,
}

impl FileInfo {
    pub fn new(name: String, absolute_path: String, size: u64, is_dir: bool) -> Self {
        Self {
            name,
            absolute_path,
            size,
            is_dir,
        }
    }
}

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

pub fn get_absolute_paths<P: AsRef<Path>>(paths: &[P]) -> Result<(Vec<PathBuf>, Vec<PathBuf>)> {
    let mut result = vec![];
    let mut not_found = vec![];
    for path in paths {
        let path_string = path.as_ref().to_string_lossy().to_string();
        if path_string.contains("*") {
            let glob_paths = glob::glob(path_string.as_ref()).unwrap();
            for glob_path in glob_paths {
                let path = glob_path.unwrap();
                let p = get_absolute_path(path.as_path());
                match p {
                    Ok(p) => {
                        result.push(p);
                    }
                    Err(e) => {
                        match e {
                            Error::FileNotFound(_) => {
                                not_found.push(path.to_path_buf());
                            }
                            _ => {
                                return Err(e);
                            }
                        }
                    }
                }
            }
        } else {
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
    }
    Ok((result, not_found))
}

pub fn get_files_info<P: AsRef<Path>>(paths: &[P]) -> Result<Vec<FileInfo>> {
    let mut files_info = vec![];
    for path in paths {
        let path = path.as_ref();
        if let Ok(metadata) = std::fs::metadata(path) {
            if metadata.is_file() {
                let name = path.file_name().unwrap().to_string_lossy().to_string();
                let size = metadata.len();
                let abs_path = path.to_string_lossy().to_string();
                let file_info = FileInfo::new(name, abs_path, size, metadata.is_dir());
                files_info.push(file_info);
            } else {
                let walk_dir = walkdir::WalkDir::new(path);
                let it = &mut walk_dir.into_iter().filter_map(|e| e.ok());
                for entry in it {
                    let path = entry.path();
                    if let Ok(metadata) = std::fs::metadata(path) {
                        let name = path.file_name().unwrap().to_string_lossy().to_string();
                        let size = metadata.len();
                        let abs_path = path.to_string_lossy().to_string();
                        let file_info = FileInfo::new(name, abs_path, size, metadata.is_dir());
                        files_info.push(file_info);
                    }
                }
            }
        }
    }
    Ok(files_info)
}