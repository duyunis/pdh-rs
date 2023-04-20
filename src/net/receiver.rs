use std::{mem, thread};
use std::collections::HashMap;
use std::future::Future;
use std::io::ErrorKind;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use log::{debug, error, warn};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::runtime::Runtime;
use tokio::sync::broadcast;
use tokio::sync::broadcast::{Receiver, Sender};
use tokio::task::JoinHandle;

use crate::message::{self, Message, RecvAble, SendAble};

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ReceiverConfig {
    pub listener_port: u16,
    pub send_max_buffer: usize,
}

pub type Bidirectional<T> = fn(Sender<SendAble<T>>, Receiver<RecvAble>) -> Pin<Box<dyn Future<Output=anyhow::Result<()>> + Send>>;

pub struct UReceiver<T> {
    name: &'static str,
    config: ReceiverConfig,
    runtime: Arc<Runtime>,
    threads_handle: Mutex<Vec<JoinHandle<()>>>,
    running: Arc<AtomicBool>,
    bidirectional: Arc<Bidirectional<T>>,
}

impl<T: Message> UReceiver<T> {
    pub fn new(
        name: &'static str,
        config: ReceiverConfig,
        runtime: Arc<Runtime>,
        running: Arc<AtomicBool>,
        bidirectional: Bidirectional<T>,
    ) -> Self {
        let threads_handle = Mutex::default();
        Self {
            name,
            config,
            runtime,
            threads_handle,
            running,
            bidirectional: Arc::new(bidirectional),
        }
    }

    pub async fn process(&mut self) {
        while self.running.load(Ordering::Relaxed) {}
    }

    pub async fn listener(&self) {
        let addr = format!("0.0.0.0:{}", self.config.listener_port);
        let listener = TcpListener::bind(addr.as_str()).await;
        match listener {
            Ok(listener) => {
                while self.running.load(Ordering::Relaxed) {
                    let incoming = listener.accept().await;
                    if let Ok((mut stream, addr)) = incoming {
                        let running = self.running.clone();
                        let bidirectional = self.bidirectional.clone();
                        let send_max_buffer = self.config.send_max_buffer.clone();

                        let (sender_send, sender_recv) = broadcast::channel(1024);
                        let (recv_send, mut recv_recv) = broadcast::channel(1024);
                        let recv_send = Arc::new(recv_send);

                        self.threads_handle
                            .lock()
                            .unwrap()
                            .push(self.runtime.spawn(async move {
                                Self::process_stream(running, stream, send_max_buffer, sender_recv, recv_send).await;
                            }));

                        self.threads_handle
                            .lock()
                            .unwrap()
                            .push(self.runtime.spawn(async move {
                                bidirectional(sender_send, recv_recv).await.unwrap();
                            }));
                    }
                }
            }
            Err(e) => {
                self.running.store(false, Ordering::Relaxed);
                error!("start receiver failed: {}", e);
            }
        }
    }

    async fn process_stream(running: Arc<AtomicBool>, mut stream: TcpStream, send_max_buffer: usize, mut input: Receiver<SendAble<T>>, output: Arc<Sender<RecvAble>>) {
        let header_size = message::Header::header_size();
        let mut header_buf = vec![0u8; header_size];
        while running.load(Ordering::Relaxed) {
            tokio::select! {
                Ok(mut send_able) = input.recv() => {
                    let mut buffer = vec![];
                    let res = send_able.encode(&mut buffer);
                    match res {
                        Ok(_) => {
                            for buf in buffer.chunks(send_max_buffer) {
                                let result = stream.write(buf).await;
                                match result {
                                    Ok(size) => {
                                        stream.flush().await.unwrap();
                                    }
                                    Err(e) if e.kind() == ErrorKind::WouldBlock => {
                                        debug!("tcp stream write data block {}", e);
                                        continue;
                                    }
                                    Err(e) => {
                                        error!("tcp stream write data failed: {}", e);
                                        break;
                                    }
                                };
                            }
                        }
                        Err(e) => {
                            error!("encode message failed: {}", e);
                        }
                    }
                },
                Ok(n) = stream.read(header_buf.as_mut_slice()) => {
                    match n {
                        0 => {
                            println!("client is closed");
                            break;
                        }
                        _ => {
                            let header = message::Header::decode(header_buf.as_slice());
                            if let Ok(header) = header {
                                let mut message_buf = vec![0u8; header.frame_size as usize];
                                let read = stream.read(message_buf.as_mut_slice()).await;
                                match read {
                                    Ok(_) => {
                                        let recv_able = RecvAble::new(header.message_type, message_buf);
                                        output.send(recv_able).unwrap();
                                    }
                                    Err(e) => {
                                        error!("stream read error: {}", e);
                                    }
                                }
                            } else {
                                // print log
                            }
                        }
                    }
                },
            }
        }
    }
}
