use std::{mem, thread};
use std::collections::HashMap;
use std::future::Future;
use std::io::ErrorKind;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use log::{debug, error, info, warn};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::runtime::Runtime;
use tokio::sync::broadcast;
use tokio::sync::broadcast::{Receiver, Sender};
use tokio::task::JoinHandle;

use crate::message::{self, Message, TransmitAble};

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ReceiverConfig {
    pub listener_port: u16,
    pub send_max_buffer: usize,
}

pub type Bidirectional = fn(Sender<TransmitAble>, Receiver<TransmitAble>) -> Pin<Box<dyn Future<Output=anyhow::Result<()>> + Send>>;

pub struct UReceiver {
    name: &'static str,
    config: ReceiverConfig,
    runtime: Arc<Runtime>,
    threads_handle: Mutex<Vec<JoinHandle<()>>>,
    running: Arc<AtomicBool>,
    bidirectional: Arc<Bidirectional>,
}

impl UReceiver {
    pub fn new(
        name: &'static str,
        config: ReceiverConfig,
        runtime: Arc<Runtime>,
        running: Arc<AtomicBool>,
        bidirectional: Bidirectional,
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

    async fn process_stream(running: Arc<AtomicBool>, mut stream: TcpStream, send_max_buffer: usize, mut input: Receiver<TransmitAble>, output: Arc<Sender<TransmitAble>>) {
        let header_size = message::Header::header_size();
        let mut header_buf = vec![0u8; header_size];
        while running.load(Ordering::Relaxed) {
            tokio::select! {
                Ok(mut transmit_able) = input.recv() => {
                    let mut buffer = transmit_able.buf;
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
                },
                Ok(n) = stream.read(header_buf.as_mut_slice()) => {
                    match n {
                        0 => {
                            info!("client is closed");
                            break;
                        }
                        _ => {
                            let mut transmit_able = TransmitAble::new();

                            let header = message::Header::decode(header_buf.as_slice());
                            match header {
                                Ok(header) => {
                                    if header.frame_size == 0 {
                                        let mut message_buf = vec![];
                                        transmit_able.encode_with_header(header, message_buf.as_mut());
                                        output.send(transmit_able).unwrap();
                                    } else {
                                        let mut message_buf = vec![0u8; header.frame_size as usize];
                                        let read = stream.read(message_buf.as_mut_slice()).await;
                                        match read {
                                            Ok(_) => {
                                                let mut transmit_able = TransmitAble::new();
                                                transmit_able.encode_with_header(header, &mut message_buf);
                                                output.send(transmit_able).unwrap();
                                            }
                                            Err(e) => {
                                                error!("stream read error: {}", e);
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    error!("decode header error: {:?}", e);
                                }
                            }
                        }
                    }
                },
            }
        }
    }
}
