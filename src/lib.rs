use std::{
    collections::HashMap,
    io,
};
use log::{warn};

use futures::{
    prelude::*,
    AsyncWrite,
};

pub mod websocket;
pub mod cookies;

const NEWLINE: &[u8] = b"\r\n";

#[derive(Debug)]
pub struct Request<'a> {
    pub method: String,
    pub path: String,
    // Returns a mapping of header => (first_value, other values)
    pub headers: HashMap<&'a str, (&'a [u8], Option<Vec<&'a [u8]>>)>,
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

/// populates the provided buffer with bytes from the stream.
async fn populate_buffer<S>(stream: &mut S, buf: &mut [u8]) -> std::io::Result<usize>
where S: AsyncRead + Unpin
{
    let mut lines = 0;
    let mut i = 0;
    let mut j;
    let mut last_newline_at = 0;
    'read_loop: loop {
        j = i+1;
        // read one byte
        let count = stream.read(&mut buf[i..j]).await?;
        if count == 0 {
            // this will likely only happen if the client disconnects before header is sent
            break;
        }
        // if the byte we read was a newline, extract logic
        if buf[i] == b'\n' {
            if i - last_newline_at < 3 {
                // we might be at the end; check if last_newline_at..j is a terminal case
                let part = &buf[last_newline_at..j];
                if part == b"\n\r\n" || part == b"\n\n" {
                    break 'read_loop;
                }
            }
            lines += 1;
            last_newline_at = i;
        }
        i += 1;
        if i == buf.len() {
            break 'read_loop;
        }
    }
    Ok(lines)
}

/// Parses a stream for the http request; this does not parse the body at all,
/// so it will remain entirely intact in stream.
/// 
/// I strongly recommend you use a BufReader for the input stream. The size of the
/// provided buffer bounds the maximum number/length of the headers, so don't be too
/// stingy with it.
pub async fn http<'a, S>(stream: &mut S, buf: &'a mut [u8]) -> std::io::Result<Request<'a>>
where S: AsyncRead + Unpin
{
    let lines = populate_buffer(stream, buf).await?;
    if lines == 0 {
        // if the client disconnects before finishing the first line, we might have a problem
        return Err(io::ErrorKind::InvalidInput.into());
    }
    // 1 status line, then a buncha headers
    let mut raw_headers = vec![httparse::EMPTY_HEADER; lines - 1];
    let mut req = httparse::Request::new(&mut raw_headers);
    let res = req.parse(buf).or(Err(io::ErrorKind::InvalidInput))?;
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
    let mut headers: HashMap<&str, (&[u8], Option<Vec<&[u8]>>)> = HashMap::default();
    for header in req.headers {
        if let Some(existing) = headers.get_mut(header.name) {
            let v = existing.1.get_or_insert(vec!());
            v.push(header.value);
        } else {
            headers.insert(header.name, (header.value, None));
        }
    }
    // Convert the response to a request and return
    let request = Request{
        method: String::from(req.method.unwrap_or("GET")),
        path: String::from(req.path.unwrap_or("/")),
        headers,
    };
    //info!("HTTP/1.1 {method} {path}", method=request.method, path=request.path);
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

// send_content writes all of the contents to the specified stream. Use this to send
// body contents (such as a HTML page).
pub async fn send_content<S>(writer: &mut S, contents: &[u8]) -> io::Result<()>
where S: AsyncWrite + Unpin
{
    let mut offset = 0;
    while let Ok(count) = writer.write(&contents[offset..]).await {
        offset += count;
        if offset == contents.len() {
            break;
        }
    }
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
                let mut buf = vec![0; 64000];
                let req = http(&mut reader, &mut buf).await.unwrap();
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

    #[async_std::test]
    async fn test_server_parses_headers() -> Result<(), Box<dyn Error>> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let local_addr = listener.local_addr().unwrap();
        let handle = task::spawn(async move {
            let mut incoming = listener.incoming();
            while let Some(stream) = incoming.next().await {
                let stream = stream.unwrap();
                let mut reader = BufReader::new(stream.clone());
                let mut writer = BufWriter::new(stream);
                let mut buf = vec![0; 65536];
                let req = http(&mut reader, &mut buf).await.unwrap();
                println!("req: {:?}", req);
                assert_eq!(req.method, "GET");
                assert_eq!(req.path, "/");
                assert_eq!(req.headers.len(), 4);
                // Response
                respond(&mut writer, Response{
                    code: 200,
                    reason: "OK",
                    headers: vec!(),
                }).await.unwrap();
                break;
            }
        });
        // Make a simple HTTP request with some other library
        let path = format!("http://localhost:{}", local_addr.port());
        ureq::get(&path)
            .set("Transfer-Encoding", "chunked")
            .call();
        handle.await;
        Ok(())
    }

    // TODO: test large messages
}