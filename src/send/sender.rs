use std::io::BufRead;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use tokio::runtime::Runtime;
use tokio::task::JoinHandle;

use crate::print_flush;
use crate::utils::files;

#[derive(Clone, Debug)]
pub struct SenderConfig {
    share_code: Option<String>,
    zip: bool,
    relay: Option<String>,
    files: Vec<String>,
}

impl SenderConfig {
    pub fn new(share_code: Option<String>, zip: bool, relay: Option<String>, files: Vec<String>) -> Self {
        Self {
            share_code,
            zip,
            relay,
            files,
        }
    }
}

pub struct Sender {
    config: SenderConfig,
    runtime: Arc<Runtime>,
    threads_handle: Mutex<Vec<JoinHandle<()>>>,
}

impl Sender {
    pub fn new(runtime: Arc<Runtime>, config: SenderConfig) -> Self {
        Self {
            config,
            runtime,
            threads_handle: Mutex::default(),
        }
    }

    pub fn start_send(&self) -> Result<()> {
        print_flush!("\rStart collecting files...");
        std::thread::sleep(std::time::Duration::from_secs(1));
        let result = files::get_absolute_paths(send.files);
        match result {
            Ok((path, not_found)) => {
                // println!("collect path: {:?}, not found path: {:?}", path, not_found);
                if path.is_empty() && !not_found.is_empty() {
                    print_flush!("\r{:?}: No such file or directory.", not_found);
                    println!();
                    return Ok(());
                } else if !path.is_empty() && !not_found.is_empty() {
                    print_flush!("\r{:?}: No such file or directory.", not_found);
                    println!();
                    println!("Continue send: {:?}?\n    Is this ok [y/n]", path);
                    let stdin = std::io::stdin();
                    let mut input = String::new();
                    loop {
                        stdin.lock().read_line(&mut input).unwrap();
                        let choose = input.trim().to_lowercase();
                        if choose == "y" {} else {
                            return Ok(());
                        }
                    }
                }
            }
            Err(e) => {
                println!("Error collecting files: {}", e);
                return Ok(());
            }
        }
        Ok(())
    }
}