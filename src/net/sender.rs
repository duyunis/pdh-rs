use std::io::ErrorKind;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use std::{mem, thread};

use log::{debug, error, warn};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::runtime::Runtime;
use tokio::sync::broadcast::{Receiver, Sender};
use tokio::task::JoinHandle;

use crate::message;
use crate::message::{Message, SendAble};

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SenderConfig {
    pub dest_ip: String,
    pub dest_port: u16,
    pub send_max_buffer: usize,
}

pub struct USenderThread<T> {
    name: &'static str,
    input: Receiver<SendAble<T>>,
    output: Arc<Sender<T>>,
    config: SenderConfig,
    runtime: Arc<Runtime>,
    thread_handle: Option<JoinHandle<()>>,
    running: Arc<AtomicBool>,
}

impl<T: Message> USenderThread<T> {
    pub fn new(
        name: &'static str,
        input: Receiver<SendAble<T>>,
        output: Arc<Sender<T>>,
        config: SenderConfig,
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
            warn!("{} sender already started, do nothing.", self.name);
            return;
        }
        let input = self.input.resubscribe();
        let mut u_sender = USender::new(
            self.name,
            input,
            self.output.clone(),
            self.config.clone(),
            self.running.clone(),
        );
        self.thread_handle = Some(self.runtime.spawn(async move {
            u_sender.process().await;
        }));
        debug!("{} uniform sender started", self.name);
    }

    pub fn notify_stop(&mut self) -> Option<JoinHandle<()>> {
        if !self.running.swap(false, Ordering::Relaxed) {
            warn!("sender name: {} already stopped, do nothing.", self.name);
            return None;
        }
        debug!("notified stopping sender name: {}", self.name);
        self.thread_handle.take()
    }

    pub fn stop(&mut self) {
        if !self.running.swap(false, Ordering::Relaxed) {
            warn!("sender name: {} already stopped, do nothing.", self.name);
            return;
        }
        debug!("stopping sender name: {}", self.name);
        self.runtime.block_on(async {
            let _ = self.thread_handle.take().unwrap().await;
        });
        debug!("stopped sender name: {}", self.name);
    }
}

pub struct USender<T> {
    name: &'static str,
    input: Receiver<SendAble<T>>,
    output: Arc<Sender<T>>,
    tcp_stream: Option<TcpStream>,
    dst_ip: String,
    dst_port: u16,
    config: SenderConfig,
    reconnect: bool,
    running: Arc<AtomicBool>,
}

impl<T: Message> USender<T> {
    pub fn new(
        name: &'static str,
        input: Receiver<SendAble<T>>,
        output: Arc<Sender<T>>,
        config: SenderConfig,
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

    pub async fn process(&mut self) {
        while self.running.load(Ordering::Relaxed) {
            match self.input.recv().await {
                Ok(mut message) => {
                    self.send_message(message).await;
                }
                Err(_e) => {}
            }
        }
    }

    pub async fn out(&mut self) {
        while self.running.load(Ordering::Relaxed) {
            if self.reconnect || self.tcp_stream.is_none() {
                if let Some(mut t) = self.tcp_stream.take() {
                    if let Err(e) = t.shutdown().await {
                        debug!("{} sender tcp stream shutdown failed {}", self.name, e);
                    }
                }
                self.tcp_stream = TcpStream::connect((self.dst_ip.clone(), self.dst_port))
                    .await
                    .ok();
                if let Some(tcp_stream) = self.tcp_stream.as_mut() {
                    self.reconnect = false;
                } else {
                    error!("connect failed.");
                    return;
                }
            }
            let header_size = message::Header::header_size();
            let tcp_stream = self.tcp_stream.as_mut().unwrap();
            let mut header_buf = vec![0u8; header_size];
            let n = tcp_stream.read(header_buf.as_mut_slice()).await;
            if let Ok(_) = n {
                let header = message::Header::decode(header_buf.as_slice());
                if let Ok(header) = header {
                    let mut message_buf = vec![0u8; header.frame_size as usize];
                    let n = tcp_stream.read(message_buf.as_mut_slice()).await;
                    match header.message_type {
                        message::MessageType::Compress => {}
                        _ => {}
                    }
                } else {
                    // print log
                }
            }
        }
    }

    async fn send_message(&mut self, mut send_able: SendAble<T>) {
        if self.reconnect || self.tcp_stream.is_none() {
            if let Some(mut t) = self.tcp_stream.take() {
                if let Err(e) = t.shutdown().await {
                    debug!("{} sender tcp stream shutdown failed {}", self.name, e);
                }
            }
            self.tcp_stream = TcpStream::connect((self.dst_ip.clone(), self.dst_port))
                .await
                .ok();
            if let Some(tcp_stream) = self.tcp_stream.as_mut() {
                self.reconnect = false;
            } else {
                error!("connect failed.");
                return;
            }
        }

        let tcp_stream = self.tcp_stream.as_mut().unwrap();

        let mut buffer = vec![];
        let res = send_able.encode(&mut buffer);
        match res {
            Ok(_) => {
                let send_max_buffer = self.config.send_max_buffer;
                for buf in buffer.chunks(send_max_buffer) {
                    let result = tcp_stream.write(buf).await;
                    match result {
                        Ok(size) => {
                            tcp_stream.flush().await.unwrap();
                        }
                        Err(e) if e.kind() == ErrorKind::WouldBlock => {
                            debug!("{} sender tcp stream write data block {}", self.name, e);
                            continue;
                        }
                        Err(e) => {
                            error!(
                                "{} sender tcp stream write data to {}:{} failed: {}",
                                self.name, self.dst_ip, self.dst_port, e
                            );
                            self.tcp_stream.take();
                            break;
                        }
                    };
                }
            }
            Err(e) => {
                error!("encode message failed: {}", e);
            }
        }
    }
}
