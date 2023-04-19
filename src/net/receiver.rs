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

pub struct UReceiverThread<T> {
    name: &'static str,
    input: Receiver<T>,
    output: Arc<Sender<T>>,
    config: ReceiverConfig,
    runtime: Arc<Runtime>,
    thread_handle: Option<JoinHandle<()>>,
    running: Arc<AtomicBool>,
}

impl<T: Message> UReceiverThread<T> {
    pub fn new(
        name: &'static str,
        input: Receiver<T>,
        output: Arc<Sender<T>>,
        config: ReceiverConfig,
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
            warn!("{} receiver already started, do nothing.", self.name);
            return;
        }
        let input = self.input.resubscribe();
        /*let mut u_receiver = UReceiver::new(
            self.name,
            input,
            self.output.clone(),
            self.config.clone(),
            self.runtime.clone(),
            self.running.clone(),
        );
        self.thread_handle = Some(self.runtime.spawn(async move {
            u_receiver.process().await;
        }));*/
        debug!("{} uniform receiver started", self.name);
    }

    pub fn notify_stop(&mut self) -> Option<JoinHandle<()>> {
        if !self.running.swap(false, Ordering::Relaxed) {
            warn!("receiver name: {} already stopped, do nothing.", self.name);
            return None;
        }
        debug!("notified stopping receiver name: {}", self.name);
        self.thread_handle.take()
    }

    pub fn stop(&mut self) {
        if !self.running.swap(false, Ordering::Relaxed) {
            warn!("receiver name: {} already stopped, do nothing.", self.name);
            return;
        }
        debug!("stopping receiver name: {}", self.name);
        self.runtime.block_on(async {
            let _ = self.thread_handle.take().unwrap().await;
        });
        debug!("stopped receiver name: {}", self.name);
    }
}

// pub type HandelMessage = fn(TcpStream) -> Pin<Box<dyn Future<Output=anyhow::Result<()>> + Send>>;
pub type HandelMessage<T> = fn(Sender<SendAble<T>>, Receiver<RecvAble>) -> Pin<Box<dyn Future<Output=anyhow::Result<()>> + Send>>;

pub struct UReceiver<T> {
    name: &'static str,
    input: Receiver<T>,
    output: Arc<Sender<T>>,
    incoming: Mutex<HashMap<String, Arc<TcpStream>>>,
    last_flush: Duration,
    config: ReceiverConfig,
    runtime: Arc<Runtime>,
    threads_handle: Mutex<Vec<JoinHandle<()>>>,
    running: Arc<AtomicBool>,
    handel_message: Arc<HandelMessage<T>>,
}

impl<T: Message> UReceiver<T> {
    pub fn new(
        name: &'static str,
        input: Receiver<T>,
        output: Arc<Sender<T>>,
        config: ReceiverConfig,
        runtime: Arc<Runtime>,
        running: Arc<AtomicBool>,
        handel_message: HandelMessage<T>,
    ) -> Self {
        let incoming = Mutex::default();
        let threads_handle = Mutex::default();
        Self {
            name,
            input,
            output,
            incoming,
            last_flush: Duration::ZERO,
            config,
            runtime,
            threads_handle,
            running,
            handel_message: Arc::new(handel_message),
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
                        let handel_message = self.handel_message.clone();
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
                                let res = handel_message(sender_send, recv_recv).await;
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
                                // todo match n
                                // let m = message::decode_message(header.msg_type, message_buf);
                                // let res = sender.send(BaseMessage{buffer: vec![]});
                                // let (sender, receiver) = broadcast::channel(1024);
                                // let res = (handel_message)(receiver, Arc::new(sender));
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
