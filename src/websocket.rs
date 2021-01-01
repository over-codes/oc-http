use std::{
    io,
    convert::TryFrom,
    fmt,
};

use sha1::{Sha1, Digest};
use crate::{respond, Request, Response};
use nom::{
    IResult,
    bits::{
        bits,
        complete::take,
    },
};
use futures::{
    AsyncRead,
    AsyncWrite,
    AsyncWriteExt,
    AsyncReadExt,
};

const MAX_PAYLOAD_SIZE: u64 = 16_000;

#[derive(Debug, Clone)]
pub enum WebSocketError {
    ConnectioNotUpgrade,
    NoConnectionHeader,
    NoUpgradeHeader,
    UpgradeNotToWebSocket,
    WrongVersion,
    NoKey,
    TooBig,
    ProtocolError,
    IOError(String),
    BadOpcode,
    ConnectionClosed,
}

impl From<io::Error> for WebSocketError {
    fn from(err: io::Error) -> Self {
        WebSocketError::IOError(format!("{:?}", err))
    }
}

impl<E> From<nom::Err<E>> for WebSocketError {
    fn from(_err: nom::Err<E>) -> Self {
        WebSocketError::ProtocolError
    }
}

impl fmt::Display for WebSocketError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "problem establishing websocket connection")
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MessageType {
    Continuation,
    Text,
    Binary,
    Close,
    Ping,
    Pong,
}

impl MessageType {
    pub fn is_control(&self) -> bool {
        match self {
            MessageType::Ping | MessageType::Pong | MessageType::Close => true,
            _ => false,
        }
    }
}

impl TryFrom<u8> for MessageType {
    type Error = WebSocketError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        Ok(match b {
            0x0 => MessageType::Continuation,
            0x1 => MessageType::Text,
            0x2 => MessageType::Binary,
            0x8 => MessageType::Close,
            0x9 => MessageType::Ping,
            0xA => MessageType::Pong,
            _ => return Err(WebSocketError::BadOpcode),
        })
    }
}

impl Into<u8> for MessageType {
    fn into(self) -> u8 {
        match self {
            MessageType::Continuation => 0x0,
            MessageType::Text => 0x1,
            MessageType::Binary => 0x2,
            MessageType::Close => 0x8,
            MessageType::Ping => 0x9,
            MessageType::Pong => 0xA,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Message{
    pub typ: MessageType,
    pub contents: Vec<u8>,
}

pub async fn upgrade<S>(req: &Request, mut stream: S) -> Result<(WebSocketReader<S>, WebSocketWriter<S>), WebSocketError>
where S: AsyncRead + AsyncWrite + Clone + Unpin
{
    // sanity check that required headers are in place
    match req.headers.get("Connection") {
        Some(header) => if header != b"Upgrade" { Err(WebSocketError::ConnectioNotUpgrade)? },
        None => Err(WebSocketError::NoConnectionHeader)?,
    };
    match req.headers.get("Upgrade") {
        Some(header) => if header != b"websocket" { Err(WebSocketError::UpgradeNotToWebSocket)? },
        None => Err(WebSocketError::NoUpgradeHeader)?,
    };
    match req.headers.get("Sec-WebSocket-Version") {
        Some(header) => if header != b"13" { Err(WebSocketError::WrongVersion)? },
        None => Err(WebSocketError::WrongVersion)?,
    };
    // get the key we need to hash in the response
    let key = match req.headers.get("Sec-WebSocket-Key") {
        Some(k) => k,
        None => Err(WebSocketError::NoKey)?,
    };
    let mut hasher = Sha1::new();
    hasher.update(&key);
    // magic string from the interwebs
    hasher.update("258EAFA5-E914-47DA-95CA-C5AB0DC85B11");
    let result = hasher.finalize();
    let mut headers = vec!();
    headers.push(("Upgrade".into(), Vec::from("websocket")));
    headers.push(("Connection".into(), Vec::from("Upgrade")));
    headers.push(("Sec-WebSocket-Accept".into(), base64::encode(&result[..]).into()));
    // complete the handshake
    respond(&mut stream, Response{
        code: 101,
        reason: "Switching Protocols",
        headers,
    }).await?;
    stream.flush().await?;
    Ok((WebSocketReader{
        stream: stream.clone(),
        buffered_message: None,
    }, WebSocketWriter{
        stream,
    }))
}

pub struct WebSocketReader<S>
where S: AsyncRead + Unpin
{
    stream: S,
    buffered_message: Option<(MessageType, Vec<u8>)>,
}

impl<S> WebSocketReader<S>
where S: AsyncRead + Unpin
{
    pub async fn recv(&mut self) -> Result<Message, WebSocketError> {
        loop {
            let header = read_header(&mut self.stream).await?;
            if header.payload_len > MAX_PAYLOAD_SIZE {
                Err(WebSocketError::TooBig)?;
            }
            // read the body
            let mut contents = vec![0u8; header.payload_len as usize];
            self.stream.read_exact(&mut contents).await?;
            // unmask the value in-place
            let len = contents.len();
            for i in 0..len {
                contents[i] = contents[i] ^ header.masking_key[i % header.masking_key.len()];
            }
            let typ = MessageType::try_from(header.opcode)?;
            if typ.is_control() {
                return Ok(Message{contents, typ});
            }
            // if this is a new fragment chain, start it
            if header.fin == 0 && typ != MessageType::Continuation {
                self.buffered_message = Some((typ, contents));
            } else if header.fin == 0 {
                match &mut self.buffered_message {
                    Some((_, old)) => {
                        old.append(&mut contents);
                    },
                    None => return Err(WebSocketError::BadOpcode),
                }
            } else {
                let (typ, contents) = self.buffered_message.take().unwrap_or((typ, contents));
                return Ok(Message{typ, contents});
            }
        }
    }
}

pub struct WebSocketWriter<S>
where S: AsyncWrite + Unpin
{
    stream: S,
}

impl<S> WebSocketWriter<S>
where S: AsyncWrite + Unpin
{
    pub async fn write(&mut self, msg: &Message) -> Result<(), WebSocketError> {
        let res = WebSocketHeader{
            fin: 1,
            opcode: msg.typ.into(),
            mask: 0,
            payload_len: msg.contents.len() as u64,
            masking_key: vec!(),
        };
        self.stream.write_all(&mut res.to_vec()).await?;
        self.stream.write_all(&msg.contents).await?;
        self.stream.flush().await?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct WebSocketHeader{
    fin: u8,
    opcode: u8,
    mask: u8,
    payload_len: u64,
    masking_key: Vec<u8>,
}

impl WebSocketHeader {
    fn to_vec(&self) -> Vec<u8> {
        let mut ret = Vec::with_capacity(70);
        ret.push((self.fin << 7) | self.opcode);
        ret.extend(if self.payload_len < 126 {
            vec!(self.payload_len as u8)
        } else if self.payload_len < u16::MAX as u64 {
            let mut ret = vec!(126u8);
            ret.extend(&(self.payload_len as u16).to_be_bytes());
            ret
        } else {
            let mut ret = vec!(127u8);
            ret.extend(&(self.payload_len as u16).to_be_bytes());
            ret
        });
        ret
    }
}

/// handles control message (ping, pong) to make sure the socket stays open
pub async fn handle_control<S>(msg: &Message, wrt: &mut WebSocketWriter<S>) -> Result<bool, WebSocketError>
where S: AsyncWrite + Unpin
{
    match msg.typ {
        MessageType::Pong => {
            let msg = Message{
                typ: MessageType::Pong,
                contents: msg.contents.clone(),
            };
            wrt.write(&msg).await?;
            Ok(true)
        },
        MessageType::Close => {
            Err(WebSocketError::ConnectionClosed)
        }
        _ => {
            Ok(false)
        },
    }
}

async fn read_header<S>(stream: &mut S) -> Result<WebSocketHeader, WebSocketError>
where S: AsyncRead + Unpin
{
    // fixed-length header size is 2 bytes, followed by optional extended length
    // and finally mask
    let mut header_fixed = vec![0u8; 2];
    stream.read_exact(&mut header_fixed).await?;
    let (_, mut res) = read_header_internal(&header_fixed)?;
    header_fixed[1] &= 0b01111111;
    if res.payload_len == 126 {
        // read 16 bites, 2 bytes
        let mut len = [0u8; 2];
        stream.read_exact(&mut len).await?;
        res.payload_len = u16::from_be_bytes(len) as u64;
    } else if res.payload_len == 127 {
        // read 64 bits, 8 bytes
        let mut len = [0u8; 8];
        stream.read_exact(&mut len).await?;
        res.payload_len = u64::from_be_bytes(len) as u64;
    }
    if res.mask != 0 {
        let mut mask_key = vec![0u8; 4];
        stream.read_exact(&mut mask_key).await?;
        res.masking_key =  mask_key;
    }
    Ok(res)
}

fn read_header_internal(input: &[u8]) -> IResult<&[u8], WebSocketHeader> {
    bits(read_header_internal_bits)(input)
}

fn read_header_internal_bits(input: (&[u8], usize)) -> IResult<(&[u8], usize), WebSocketHeader>
{
    let (input, fin) = take(1usize)(input)?;
    let (input, _rsv1): ((&[u8], usize), u8) = take(1usize)(input)?;
    let (input, _rsv2): ((&[u8], usize), u8) = take(1usize)(input)?;
    let (input, _rsv3): ((&[u8], usize), u8) = take(1usize)(input)?;
    let (input, opcode) = take(4usize)(input)?;
    let (input, mask) = take(1usize)(input)?;
    let (input, payload_len) = take(7usize)(input)?;
    Ok((input, WebSocketHeader{fin, opcode, mask, payload_len, masking_key: vec!()}))
}