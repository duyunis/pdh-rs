use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time;

use tokio::runtime::Runtime;

use crate::common::consts;

pub struct DiscoverOptions {
    broadcast_addr: String,
    service_port: u16,
    timeout: time::Duration,
    broadcast_delay: time::Duration,
    payload: Vec<u8>,
}

impl DiscoverOptions {
    pub fn new(
        broadcast_addr: String,
        service_port: u16,
        timeout: time::Duration,
        broadcast_delay: time::Duration,
        payload: Vec<u8>,
    ) -> Self {
        Self {
            broadcast_addr,
            service_port,
            timeout,
            broadcast_delay,
            payload,
        }
    }
}

impl Default for DiscoverOptions {
    fn default() -> Self {
        Self {
            broadcast_addr: consts::DEFAULT_BROADCAST_ADDR.to_string(),
            service_port: consts::DEFAULT_SERVICE_PORT,
            timeout: time::Duration::from_secs(60 * 3),
            broadcast_delay: time::Duration::from_secs(1),
            payload: b"pdh_broadcast_msg".to_vec(),
        }
    }
}

pub struct Service {
    pub socket_addr: SocketAddr,
}

pub struct Discover {
    options: DiscoverOptions,
    rt: Arc<Runtime>,
}

impl Discover {
    pub fn new(rt: Arc<Runtime>, options: DiscoverOptions) -> Self {
        Self { options, rt }
    }

    pub fn discover_service(&self) -> Option<Service> {
        let res = self.rt.block_on(self.discover());
        match res {
            Ok(service) => {
                if let Some(service) = service {
                    return Some(service);
                }
            }
            Err(e) => {
                println!("discover service error: {}", e);
            }
        }
        None
    }

    async fn discover(&self) -> std::io::Result<Option<Service>> {
        let socket = UdpSocket::bind(format!("{}:{}", "0.0.0.0", self.options.service_port))?;
        socket.set_broadcast(true)?;
        socket.set_read_timeout(Some(time::Duration::from_secs(3)))?;

        let res = tokio::time::timeout(self.options.timeout, async {
            let mut buffer = [0; 1024];
            // retry 10 times
            for _ in 0..10 {
                let recv = socket.recv_from(&mut buffer);
                match recv {
                    Ok(recv) => {
                        let (bytes_read, src_addr) = recv;
                        let read = buffer[..bytes_read].to_vec();
                        if read.eq(&self.options.payload) {
                            return Some(Service {
                                socket_addr: src_addr,
                            });
                        }
                    }
                    Err(_) => {}
                }
            }
            None
        })
        .await;
        match res {
            Ok(service) => {
                return Ok(service);
            }
            Err(_) => {
                println!("discover service timed out!");
            }
        }

        Ok(None)
    }
}
