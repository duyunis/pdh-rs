use std::fmt;
use std::fmt::Debug;

use anyhow::Result;
use byteorder::{BigEndian, ReadBytesExt};
use num_enum::IntoPrimitive;
use serde::{Deserialize, Serialize};

/// A abstraction for message data and serialize data
pub trait Message: Debug + Send + Clone + 'static {
    // Encode message to bytes stream
    fn encode(&self) -> Result<Vec<u8>>;

    // The message type identified by the sender
    fn message_type(&self) -> MessageType;
}

// Decode message from bytes stream
pub fn decode<'de, T>(buf: &'de [u8]) -> Result<T>
    where
        T: Message,
        T: Deserialize<'de>,
{
    let msg = bincode::deserialize(buf)?;
    Ok(msg)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaseMessage {
    pub buf: Vec<u8>,
}

impl BaseMessage {
    pub fn new(buf: Vec<u8>) -> Self {
        Self {
            buf,
        }
    }
}

impl Message for BaseMessage {
    fn encode(&self) -> Result<Vec<u8>> {
        let buf = bincode::serialize(self)?;
        Ok(buf)
    }

    fn message_type(&self) -> MessageType {
        MessageType::Compress
    }
}

#[derive(Debug, Clone, Copy, IntoPrimitive)]
#[repr(u8)]
pub enum MessageType {
    Compress = 0,
    Syslog = 1,
    Statsd = 2,
    Metrics = 3,
    TaggedFlow = 4,
    ProtocolLog = 5,
    OpenTelemetry = 6,
    Prometheus = 7,
    Telegraf = 8,
    PacketSequenceBlock = 9,
    MicflowStats = 10,
    OpenTelemetryCompressed = 11,
    RawPcap = 12,
    HostMetric = 13,
    Profile = 14,
    ProcEvents = 15,
    Unknown = 100,
}

impl fmt::Display for MessageType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Compress => write!(f, "compress"),
            Self::Syslog => write!(f, "syslog"),
            Self::Statsd => write!(f, "statsd"),
            Self::Metrics => write!(f, "metrics"),
            Self::TaggedFlow => write!(f, "l4_log"),
            Self::ProtocolLog => write!(f, "l7_log"),
            Self::OpenTelemetry => write!(f, "open_telemetry"),
            Self::Prometheus => write!(f, "prometheus"),
            Self::Telegraf => write!(f, "telegraf"),
            Self::PacketSequenceBlock => write!(f, "packet_sequence_block"), // Enterprise Edition Feature: packet-sequence
            Self::MicflowStats => write!(f, "micflow_stats"),
            Self::OpenTelemetryCompressed => write!(f, "open_telemetry compressed"),
            Self::RawPcap => write!(f, "raw_pcap"), // Enterprise Edition Feature: pcap
            Self::HostMetric => write!(f, "host_metric"),
            Self::Profile => write!(f, "profile"),
            Self::ProcEvents => write!(f, "proc_events"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

impl From<u8> for MessageType {
    fn from(value: u8) -> Self {
        match value {
            0 => MessageType::Compress,
            1 => MessageType::Syslog,
            2 => MessageType::Statsd,
            100 | _ => MessageType::Unknown,
        }
    }
}

const HEADER_SIZE: usize = 8 + 1 + 8;

#[derive(Debug, Clone)]
pub struct Header {
    pub frame_size: u64,
    pub message_type: MessageType,
    pub offset: u64,
}

impl Header {
    pub fn new(frame_size: u64, message_type: MessageType, offset: u64) -> Self {
        Self {
            frame_size,
            message_type,
            offset,
        }
    }

    pub fn encode(&self, buffer: &mut Vec<u8>) {
        buffer.extend_from_slice(self.frame_size.to_be_bytes().as_slice());
        buffer.push(self.message_type.into());
        buffer.extend_from_slice(self.offset.to_be_bytes().as_slice());
    }

    pub fn decode(buf: &[u8]) -> Result<Header> {
        let frame_size = buf[0..8].as_ref().read_u64::<BigEndian>()?;
        let msg_type = MessageType::from(buf[8]);
        let offset = buf[9..].as_ref().read_u64::<BigEndian>()?;
        Ok(Header::new(frame_size, msg_type, offset))
    }

    pub fn header_size() -> usize {
        HEADER_SIZE
    }
}

impl Default for Header {
    fn default() -> Self {
        Self {
            frame_size: 0,
            message_type: MessageType::Unknown,
            offset: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SendAble<T> {
    pub header: Header,
    pub message: T,
}

impl<T: Message> SendAble<T> {
    pub fn new(message: T) -> Self {
        Self {
            header: Header::default(),
            message,
        }
    }

    pub fn encode(&mut self, buf: &mut Vec<u8>) -> Result<()> {
        let mut mb = self.message.encode()?;
        self.header.frame_size = mb.len() as u64;
        self.header.message_type = self.message.message_type();
        self.header.encode(buf);
        buf.append(mb.as_mut());
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct RecvAble {
    pub message_type: MessageType,
    pub buf: Vec<u8>,
}

impl RecvAble {
    pub fn new(message_type: MessageType, buf: Vec<u8>) -> Self {
        Self {
            message_type,
            buf,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TransmitAble {
    pub header: Option<Header>,
    pub buf: Vec<u8>,
}

impl TransmitAble {
    pub fn new() -> Self {
        Self {
            header: None,
            buf: vec![],
        }
    }

    pub fn encode_with_message<T: Message>(&mut self, message: T, offset: u64) -> Result<()> {
        let mut buf = vec![];
        let mut msg_buf = message.encode()?;
        let mut header = Header::new(msg_buf.len() as u64, message.message_type(), offset);
        header.encode(buf.as_mut());
        buf.append(msg_buf.as_mut());
        self.header = Some(header);
        self.buf = buf;
        Ok(())
    }

    pub fn encode_with_header(&mut self, header: Header, msg_buf: &mut Vec<u8>) {
        let mut buf = vec![];
        header.encode(buf.as_mut());
        buf.append(msg_buf.as_mut());
        self.header = Some(header);
        self.buf = buf;
    }
}
