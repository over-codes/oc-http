use std::collections::HashMap;
use std::error::Error;

use std::fmt;

//use httparse::{Response, Request};
use async_trait::async_trait;

use log::{trace, warn};
use async_std::{
    prelude::*,
    io,
    task,
    net::{
        TcpListener,
        TcpStream,
        Shutdown,
    },
    sync::Arc,
};

const MAX_BUFFER_SIZE: usize = 16_000_000;

pub struct Header {
    pub name: String,
    pub value: Vec<u8>,
}

#[derive(Debug)]
pub struct Request<'a> {
    pub method: &'a str,
    pub path: &'a str,
    pub headers: HashMap<String, Vec<u8>>,
    pub stream: TcpStream,
    pub partial_body: &'a [u8],
}


impl<'a> fmt::Display for Request<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{} {}", self.method, self.path)?;
        for (name, value) in &self.headers {
            writeln!(f, "{}: {}", name, String::from_utf8_lossy(value))?;
        }
        writeln!(f, "\n{}", String::from_utf8_lossy(self.partial_body))
    }
}


#[derive(Debug)]
pub struct Response {
    pub code: Option<usize>,
    pub reason: Option<&'static str>,
    pub headers: HashMap<&'static str, &'static [u8]>,
    pub body: Option<Vec<u8>>,
}

#[async_trait]
pub trait Server: Send + Sync {
    async fn handle<'a>(&self, request: &mut Request<'a>, response: &mut Response) -> io::Result<()>;
}

pub async fn serve<S>(server: Arc<S>, listener: TcpListener) -> Result<(), Box<dyn Error>>
where S: Server + 'static
{
    let mut incoming = listener.incoming();
    while let Some(stream) = incoming.next().await {
        let server = server.clone();
        task::spawn(handle_request_wrapper(server, stream));
    }
    Ok(())
}

async fn handle_request_wrapper<S>(server: Arc<S>, stream: io::Result<TcpStream>)
where S: Server
{
    match handle_request(server, stream).await {
        Ok(_) => (),
        Err(err) => {
            warn!("Problem handling request: {:?}", err);
            
        }
    }
}

async fn handle_request<S>(server: Arc<S>, stream: io::Result<TcpStream>) -> io::Result<()>
where S: Server
{
    let mut stream = stream?;
    let mut headers = [httparse::EMPTY_HEADER; 16];
    let mut req = httparse::Request::new(&mut headers);
    // read the message; try to parse, if fail, read more
    let mut buf = vec![0; MAX_BUFFER_SIZE];
    let mut offset = 0;
    let mut max = 0;
    while let Ok(count) = stream.read(&mut buf[offset..]).await {
        if count == 0 {
            break;
        }
        offset += count;
        let res = req.parse(&buf).or(Err(io::ErrorKind::InvalidInput))?;
        match res {
            httparse::Status::Complete(end) => {
                max = offset;
                offset = end;
                break;
            },
            httparse::Status::Partial => {
                // this results in an extra allocation, because the buffers may be polluted. It's fine...
                headers = [httparse::EMPTY_HEADER; 16];
                req = httparse::Request::new(&mut headers);
            }
        }
    }
    if req.version.unwrap_or(1) != 1 {
        // not supported
        return Err(io::ErrorKind::InvalidInput.into());
    }
    // Call into the handle code, wait for the return
    trace!("HTTP/1.1 {method} {path}", method=req.method.unwrap_or("GET"), path=req.path.unwrap_or("/"));
    let mut req_headers = HashMap::default();
    for header in req.headers {
        if !header.name.is_empty() {
            req_headers.insert(String::from(header.name), Vec::from(header.value));
        }
    }
    let mut request = Request{
        method: req.method.unwrap_or("GET"),
        path: req.path.unwrap_or("/"),
        headers: req_headers,
        stream: stream.clone(),
        partial_body: &buf[offset..max],
    };
    let mut response = Response{
        code: None,
        reason: None,
        headers: HashMap::default(),
        body: None,
    };
    server.handle(&mut request, &mut response).await?;
    let resp = format!("HTTP/1.1 {code} {reason}",
        code=format!("{}", response.code.unwrap_or(200)),
        reason=response.reason.unwrap_or("OK"),
    );
    stream.write_all(&resp.as_bytes()).await?;
    for (name, value) in &response.headers {
        stream.write_all(format!("\n{name}: ", name=name).as_bytes()).await?;
        stream.write_all(&value).await?;
    }
    stream.write_all(b"\n").await?;
    // Write the body
    if let Some(body) = response.body {
        stream.write_all(b"\n").await?;
        stream.write_all(&body).await?;
    } else {
        stream.write_all(b"\n").await?;
    }
    stream.shutdown(Shutdown::Both)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use reqwest::StatusCode;
    use super::*;

    #[tokio::test]
    async fn test_hello_world() -> Result<(), Box<dyn Error>> {
        #[derive(Clone)]
        struct TestServer{}
    
        #[async_trait]
        impl Server for TestServer {
            async fn handle<'a>(&self, _request: &mut Request<'a>, response: &mut Response) -> io::Result<()> {
                response.headers.insert("Content-Type", b"text/html; charset=utf-8");
                response.body = Some(Vec::from("<h1>Hello world!</h1>".to_string()));
                Ok(())
            }
        }

        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let local_addr = listener.local_addr().unwrap();
        let _handle = task::spawn(async {
            serve(Arc::new(TestServer{}), listener).await.unwrap()
        });
        // Make a simple HTTP request with some other library
        let path = format!("http://localhost:{}", local_addr.port());
        let res = reqwest::get(&path).await?;
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(res.headers().get("Content-Type").unwrap(), "text/html; charset=utf-8");
        assert_eq!(res.text().await.unwrap(), "<h1>Hello world!</h1>");
        Ok(())
    }

    #[tokio::test]
    async fn test_post_message() -> Result<(), Box<dyn Error>> {
        #[derive(Clone)]
        struct TestServer{}
    
        #[async_trait]
        impl Server for TestServer {
            async fn handle<'a>(&self, request: &mut Request<'a>, response: &mut Response) -> io::Result<()> {
                response.headers.insert("Content-Type", b"text/html; charset=utf-8");
                println!("message: {}", request);
                Ok(())
            }
        }

        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let local_addr = listener.local_addr().unwrap();
        let _handle = task::spawn(async {
            serve(Arc::new(TestServer{}), listener).await.unwrap()
        });
        // Make a simple HTTP request with some other library
        let path = format!("http://localhost:{}/post", local_addr.port());
        let params = [("foo", "bar"), ("baz", "quux")];
        let client = reqwest::Client::new();
        let res = client.post(&path)
            .form(&params)
            .send()
            .await?;
        assert_eq!(res.status(), StatusCode::OK);
        Ok(())
    }
}