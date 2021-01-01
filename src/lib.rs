use std::{
    collections::HashMap,
    io,
};
use log::{info, warn};

use futures::{
    prelude::*,
    AsyncBufRead,
    AsyncWrite,
};

pub mod websocket;
pub mod cookies;

const NEWLINE: &[u8] = b"\r\n";
const MAX_HEADER_LENGTH: usize = 1024;
const MAX_HEADERS: usize = 128;

#[derive(Debug)]
pub struct Request {
    pub method: String,
    pub path: String,
    pub headers: HashMap<String, Vec<u8>>,
}

#[derive(Debug)]
pub struct Response {
    pub code: usize,
    pub reason: &'static str,
    pub headers: Vec<(String, Vec<u8>)>,
}

impl Default for Response {
    fn default() -> Self {
        Response{
            code: 200,
            reason: "OK",
            headers: vec!(),
        }
    }
}

/// Parses a stream for the http request; this does not parse the body at all,
/// so it will remain entirely intact in stream.
/// 
/// There are currently some hard-coded limits on the number of headers and length
/// of each header. The length is limited by the size of MAX_HEADER_LENGTH, and the
/// number if limited by MAX_HEADERS.
pub async fn http<S>(stream: &mut S) -> std::io::Result<Request>
where S: AsyncBufRead + Unpin
{
    let mut buff = Vec::new();
    let mut offset = 0;
    let mut lines = 0;
    while let Ok(count) = stream.take(MAX_HEADER_LENGTH as u64).read_until(b'\n', &mut buff).await {
        if count == 0 {
            break;
        }
        if count < 3 && (&buff[offset..offset+count] == b"\r\n" || &buff[offset..offset+count] == b"\n") {
            break;
        }
        lines += 1;
        if lines > MAX_HEADERS {
            warn!("Request had more than {} headers; rejected", MAX_HEADERS);
            return Err(io::ErrorKind::InvalidInput.into());
        }
        offset += count;
    }
    //println!("input\n\n{}", String::from_utf8_lossy(&buff));
    // 1 status line, then a buncha headers
    let mut headers = vec![httparse::EMPTY_HEADER; lines - 1];
    let mut req = httparse::Request::new(&mut headers);
    let res = req.parse(&buff).or(Err(io::ErrorKind::InvalidInput))?;
    match res {
        httparse::Status::Complete(_) => {
            // sgtm
        },
        httparse::Status::Partial => {
            // this should never happen, since we made sure all headers were read
            return Err(io::ErrorKind::InvalidInput.into());
        }
    }
    // Accept any known version (at this time, I've only seen 1.1 and 1.0)
    if req.version.unwrap_or(1) > 2 {
        // not supported
        warn!("HTTP/1.{} request rejected; don't support that", &req.version.unwrap_or(1));
        return Err(io::ErrorKind::InvalidInput.into());
    }
    // Put any headers into a hashmap for easy access
    let mut req_headers = HashMap::default();
    for header in req.headers {
        if !header.name.is_empty() {
            req_headers.insert(String::from(header.name), Vec::from(header.value));
        }
    }
    // Convert the response to a request and return
    let request = Request{
        method: String::from(req.method.unwrap_or("GET")),
        path: String::from(req.path.unwrap_or("/")),
        headers: req_headers,
    };
    info!("HTTP/1.1 {method} {path}", method=request.method, path=request.path);
    Ok(request)
}

/// Respond writes the provided response to the stream; this should be called before
/// any part of the body is written. After being called, the body can be written
/// directly to the stream.
pub async fn respond<S>(stream: &mut S, response: Response) -> io::Result<()>
where S: AsyncWrite + Unpin
{
    let buf = format!("HTTP/1.1 {code} {reason}",
        code=format!("{}", response.code),
        reason=response.reason,
    );
    stream.write_all(&buf.as_bytes()).await?;
    for (name, value) in &response.headers {
        stream.write_all(NEWLINE).await?;
        stream.write_all(name.as_bytes()).await?;
        stream.write_all(b": ").await?;
        stream.write_all(&value).await?;
    }
    // one to end the last header/status line, and one as required by the protocol
    stream.write_all(NEWLINE).await?;
    stream.write_all(NEWLINE).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use async_std::{
        task,
        net::{
            TcpListener,
        },
        io::{
            BufReader,
            BufWriter,
        }
    };
    use super::*;

    #[async_std::test]
    async fn test_hello_world() -> Result<(), Box<dyn Error>> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let local_addr = listener.local_addr().unwrap();
        let handle = task::spawn(async move {
            let mut incoming = listener.incoming();
            while let Some(stream) = incoming.next().await {
                let stream = stream.unwrap();
                let mut reader = BufReader::new(stream.clone());
                let mut writer = BufWriter::new(stream);
                let req = http(&mut reader).await.unwrap();
                assert_eq!(req.method, "GET");
                assert_eq!(req.path, "/");
                // Response
                let mut headers = vec!();
                headers.push(("Content-Type".into(), Vec::from("text/html; charset=utf-8".as_bytes())));
                respond(&mut writer, Response{
                    code: 200,
                    reason: "OK",
                    headers,
                }).await.unwrap();
                writer.write_all(b"<h1>Hello world!</h1>").await.unwrap();
                writer.flush().await.unwrap();
                break;
            }
        });
        // Make a simple HTTP request with some other library
        let path = format!("http://localhost:{}", local_addr.port());
        let res = ureq::get(&path).call();
        handle.await;
        assert_eq!(res.status(), 200);
        assert_eq!(res.header("Content-Type").unwrap(), "text/html; charset=utf-8");
        assert_eq!(res.into_string().unwrap(), "<h1>Hello world!</h1>");
        Ok(())
    }

    // TODO: test large messages
}