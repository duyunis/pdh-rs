use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Error, Result};
use byte_unit::{AdjustedByte, Byte};
use tokio::runtime::Runtime;
use tokio::task::JoinHandle;
use crate::net::discover;

use crate::print_flush;
use crate::utils::files;
use crate::utils::files::FileInfo;

#[derive(Clone, Debug)]
pub struct SenderOptions {
    share_code: Option<String>,
    zip: bool,
    relay: Option<String>,
    files: Vec<String>,
}

impl SenderOptions {
    pub fn new(share_code: Option<String>, zip: bool, relay: Option<String>, files: Vec<String>) -> Self {
        Self {
            share_code,
            zip,
            relay,
            files,
        }
    }
}

#[derive(Debug)]
pub struct Sender {
    options: SenderOptions,
    file_info: Vec<FileInfo>,
    file_total_size: AdjustedByte,
    folder_number: u64,
    file_number: u64,
    runtime: Arc<Runtime>,
    threads_handle: Mutex<Vec<JoinHandle<()>>>,
}

impl Sender {
    pub fn new(runtime: Arc<Runtime>, options: SenderOptions) -> Self {
        Self {
            options,
            file_info: vec![],
            file_total_size: Byte::default().get_appropriate_unit(false),
            folder_number: 0,
            file_number: 0,
            runtime,
            threads_handle: Mutex::default(),
        }
    }

    pub fn send(&mut self) -> Result<()> {
        self.prepare_send()?;
        let discover = discover::Discover::new(self.runtime.clone(), discover::DiscoverOptions::default());
        discover.broadcast()?;
        Ok(())
    }

    fn prepare_send(&mut self) -> Result<()> {
        let result = files::get_absolute_paths(self.options.files.as_ref());
        match result {
            Ok((path, not_found)) => {
                if path.is_empty() && !not_found.is_empty() {
                    print_flush!("\r{:?}: No such file or directory.", not_found);
                    println!();
                    return Ok(());
                } else if !path.is_empty() && !not_found.is_empty() {
                    print_flush!("\r{:?}: No such file or directory.\n", not_found);
                    println!("Continue send: {:?}?\n    Is this ok [y/n]", path);
                    let stdin = std::io::stdin();
                    let mut input = String::new();
                    loop {
                        stdin.lock().read_line(&mut input).unwrap();
                        let choose = input.trim().to_lowercase();
                        return if choose == "y" {
                            // start collecting
                            self.start_collect(path.as_ref())
                        } else {
                            Ok(())
                        };
                    }
                }
                // start collecting
                return self.start_collect(path.as_ref());
            }
            Err(e) => {
                println!("Error collecting files: {}", e);
            }
        };
        Ok(())
    }

    fn start_collect<P: AsRef<Path>>(&mut self, paths: &[P]) -> Result<()> {
        print_flush!("Start collecting files...");
        let files_info = files::get_files_info(paths).unwrap();
        let folder_number = files_info.iter().filter(|f| f.is_dir).count();
        let file_number = files_info.len() - folder_number;
        let file_total_size: u64 = files_info.iter().map(|f| f.size).sum();
        let file_total_size = Byte::from_bytes(file_total_size as u128).get_appropriate_unit(false);
        self.file_info = files_info;
        self.folder_number = folder_number as u64;
        self.file_number = file_number as u64;
        self.file_total_size = file_total_size;
        if self.folder_number > 0 {
            print_flush!("\rSending ({}) files and ({}) folders ({})", self.file_number, self.folder_number, self.file_total_size);
        } else {
            print_flush!("\rSending ({}) files ({})", self.file_number, self.file_total_size);
        }
        println!();
        Ok(())
    }
}