use std::{mem, thread};
use std::io::ErrorKind;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use log::{debug, error, info, warn};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::runtime::Runtime;
use tokio::sync::broadcast::{Receiver, Sender};
use tokio::task::JoinHandle;

use crate::message;
use crate::message::{BaseMessage, Message, MessageType, PingMessage, TransmitAble};

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ClientConfig {
    pub dest_ip: String,
    pub dest_port: u16,
    pub send_max_buffer: usize,
    pub heartbeat: Duration,
}

pub struct ClientThread {
    name: &'static str,
    input: Receiver<TransmitAble>,
    output: Arc<Sender<TransmitAble>>,
    config: ClientConfig,
    runtime: Arc<Runtime>,
    thread_handle: Option<JoinHandle<()>>,
    running: Arc<AtomicBool>,
}

impl ClientThread {
    pub fn new(
        name: &'static str,
        input: Receiver<TransmitAble>,
        output: Arc<Sender<TransmitAble>>,
        config: ClientConfig,
        runtime: Arc<Runtime>,
    ) -> Self {
        let running = Arc::new(AtomicBool::new(false));
        Self {
            name,
            input,
            output,
            config,
            runtime,
            thread_handle: None,
            running,
        }
    }

    pub fn start(&mut self) {
        if self.running.swap(true, Ordering::Relaxed) {
            warn!("{} client already started, do nothing.", self.name);
            return;
        }
        let input = self.input.resubscribe();
        let mut client = Client::new(
            self.name,
            input,
            self.output.clone(),
            self.config.clone(),
            self.running.clone(),
        );
        let input = self.input.resubscribe();
        let output = self.output.clone();
        self.thread_handle = Some(self.runtime.spawn(async move {
            client.start(input, output).await;
        }));
        debug!("{} uniform client started", self.name);
    }

    pub fn notify_stop(&mut self) -> Option<JoinHandle<()>> {
        if !self.running.swap(false, Ordering::Relaxed) {
            warn!("client name: {} already stopped, do nothing.", self.name);
            return None;
        }
        debug!("notified stopping client name: {}", self.name);
        self.thread_handle.take()
    }

    pub fn stop(&mut self) {
        if !self.running.swap(false, Ordering::Relaxed) {
            warn!("client name: {} already stopped, do nothing.", self.name);
            return;
        }
        debug!("stopping client name: {}", self.name);
        self.runtime.block_on(async {
            let _ = self.thread_handle.take().unwrap().await;
        });
        debug!("stopped client name: {}", self.name);
    }
}

pub struct Client {
    name: &'static str,
    input: Receiver<TransmitAble>,
    output: Arc<Sender<TransmitAble>>,
    tcp_stream: Option<TcpStream>,
    dst_ip: String,
    dst_port: u16,
    config: ClientConfig,
    reconnect: bool,
    running: Arc<AtomicBool>,
}

impl Client {
    pub fn new(
        name: &'static str,
        input: Receiver<TransmitAble>,
        output: Arc<Sender<TransmitAble>>,
        config: ClientConfig,
        running: Arc<AtomicBool>,
    ) -> Self {
        Self {
            name,
            input,
            output,
            tcp_stream: None,
            dst_ip: config.dest_ip.clone(),
            dst_port: config.dest_port,
            config,
            reconnect: false,
            running,
        }
    }

    async fn start(&mut self, mut input: Receiver<TransmitAble>, output: Arc<Sender<TransmitAble>>) {
        // connect to server
        self.connect().await;

        match self.tcp_stream.take() {
            Some(mut stream) => {
                let mut interval = tokio::time::interval(self.config.heartbeat);
                let header_size = message::Header::header_size();
                let mut header_buf = vec![0u8; header_size];

                while self.running.load(Ordering::Relaxed) {
                    tokio::select! {
                        Ok(mut transmit_able) = input.recv() => {
                            stream = self.send(stream, transmit_able).await.unwrap();
                        },
                        Ok(n) = stream.read(header_buf.as_mut_slice()) => {
                            match n {
                                0 => {
                                    info!("server is closed");
                                    break;
                                }
                                _ => {
                                    let header = message::Header::decode(header_buf.as_slice());
                                    if let Ok(header) = header {
                                        let mut message_buf = vec![0u8; header.frame_size as usize];
                                        let read = stream.read(message_buf.as_mut_slice()).await;
                                        match read {
                                            Ok(_) => {
                                                let mut transmit_able = TransmitAble::new();
                                                transmit_able.encode_with_header(header, &mut message_buf);
                                                output.send(transmit_able).unwrap();
                                            }
                                            Err(e) => {
                                                error!("{} stream read error: {}", self.name, e);
                                            }
                                        }
                                    } else {
                                        // print log
                                    }
                                }
                            }
                        },
                        _ = interval.tick() => {
                            debug!("heartbeat!!!");
                            let mut transmit_able = TransmitAble::new();
                            transmit_able.encode_with_message(PingMessage::new(), 0).unwrap();
                            stream = self.send(stream, transmit_able).await.unwrap();
                        }
                    }
                }
            }
            None => {}
        }
    }

    async fn send(&mut self, mut tcp_stream: TcpStream, transmit_able: TransmitAble) -> Option<TcpStream> {
        if self.reconnect || self.tcp_stream.is_none() {
            if let Some(mut t) = self.tcp_stream.take() {
                if let Err(e) = t.shutdown().await {
                    debug!("{} client tcp stream shutdown failed {}", self.name, e);
                }
            }
            self.tcp_stream = TcpStream::connect((self.dst_ip.clone(), self.dst_port))
                .await
                .ok();
            if let Some(tcp_stream) = self.tcp_stream.as_mut() {
                self.reconnect = false;
            } else {
                error!("connect failed.");
                return None;
            }
        }
        let mut buffer = transmit_able.buf;
        let send_max_buffer = self.config.send_max_buffer;
        for buf in buffer.chunks(send_max_buffer) {
            let result = tcp_stream.write(buf).await;
            match result {
                Ok(size) => {
                    tcp_stream.flush().await.unwrap();
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => {
                    debug!("{} client tcp stream write data block {}", self.name, e);
                    continue;
                }
                Err(e) => {
                    error!("{} client tcp stream write data to {}:{} failed: {}",self.name, self.dst_ip, self.dst_port, e);
                    self.tcp_stream.take();
                    break;
                }
            };
        }
        Some(tcp_stream)
    }

    async fn connect(&mut self) {
        if self.reconnect || self.tcp_stream.is_none() {
            if let Some(mut t) = self.tcp_stream.take() {
                if let Err(e) = t.shutdown().await {
                    debug!("{} client tcp stream shutdown failed {}", self.name, e);
                }
            }
            self.tcp_stream = TcpStream::connect((self.dst_ip.clone(), self.dst_port)).await.ok();
            if let Some(tcp_stream) = self.tcp_stream.as_mut() {
                self.reconnect = false;
            } else {
                error!("connect failed.");
                return;
            }
        }
    }
}
