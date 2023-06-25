use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;
use tokio::runtime::Runtime;
use tokio::sync::broadcast;
use tokio::sync::broadcast::{Receiver, Sender};

use crate::message::{MessageType, PongMessage, TransmitAble};
use crate::net::server::{self, Server};

pub struct Relay {
    port: u16,
    host: String,
    runtime: Arc<Runtime>,
    running: Arc<AtomicBool>,
}

impl Relay {
    pub fn new(host: String, port: u16, runtime: Arc<Runtime>) -> Self {
        let running = Arc::new(AtomicBool::new(false));
        Self {
            host,
            port,
            runtime,
            running,
        }
    }

    pub fn run(&self) {
        if self.running.swap(true, Ordering::Relaxed) {
            println!("relay already started, do nothing.");
            return;
        }
        let server = Server::new("relay server", server::ServerConfig { host: self.host.clone(), listener_port: self.port, send_max_buffer: 20480 }, self.runtime.clone(), self.running.clone(), handel_message);
        self.runtime.block_on(server.listener());
    }

    pub fn stop(&self) {
        if !self.running.swap(false, Ordering::Relaxed) {
            println!("relay already stopped, do nothing.");
        }
    }
}

fn handel_message(sender: Sender<TransmitAble>, mut recv: Receiver<TransmitAble>) -> Pin<Box<dyn Future<Output=anyhow::Result<()>> + Send>> {
    Box::pin(async move {
        loop {
            let recv = recv.recv().await;
            match recv {
                Ok(recv) => {
                    let transmit_able = recv.decode();
                    match transmit_able {
                        Ok((header, message)) => {
                            match header.message_type {
                                MessageType::Ping => {
                                    println!("recv ping: {:?}", message);
                                    let pong = PongMessage::new();
                                    let mut transmit_able = TransmitAble::new();
                                    transmit_able.encode_with_message(pong, 0).unwrap();
                                    sender.send(transmit_able).unwrap();
                                }
                                MessageType::Metrics => {

                                }
                                _ => {
                                    println!("unknown message type: {:?}", header.message_type);
                                }
                            }
                        }
                        Err(e) => {
                            println!("get header error: {}", e);
                        }
                    }
                }
                Err(e) => {
                    if e == broadcast::error::RecvError::Closed {
                        println!("channel is closed");
                        break;
                    }
                    println!("recv error: {}", e);
                    continue;
                }
            }
        }
        Ok(())
    })
}