use std::collections::HashMap;
use std::fmt;

use async_trait::async_trait;
use log::{info, warn};
use async_std::{
    prelude::*,
    io,
    task,
    net::{
        TcpListener,
        TcpStream,
    },
    sync::{
        Arc,
    },
};

mod stopper;

use stopper::*;

const NEWLINE: &[u8] = b"\r\n";
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

pub struct HttpServer {
    stop_token: StopToken,
    stopper: Stopper,
}

impl HttpServer {
    pub fn new() -> Self {
        let (stopper, stop_token) = Stopper::new();
        HttpServer {
            stopper,
            stop_token,
        }
    }
    pub fn shutdown(&self) {
        self.stopper.shutdown();
    }

    pub fn spawn<S>(&self, server: Arc<S>, listener: TcpListener) -> task::JoinHandle<()>
    where S: Server + 'static
    {
        let stop_token = self.stop_token.clone();
        task::spawn(serve_internal(server, listener, stop_token))
    }
}

pub async fn serve<S>(server: Arc<S>, listener: TcpListener)
where S: Server + 'static
{
    let (stopper, stop_token) = Stopper::new();
    serve_internal(server, listener, stop_token).await;
    stopper.shutdown();
}

async fn serve_internal<S>(server: Arc<S>, listener: TcpListener, stop_token: StopToken)
where S: Server + 'static
{
    let mut incoming = listener.incoming();
    while let Some(stream) = incoming.next().race(stop_token.wait()).await {
        let server = server.clone();
        task::spawn(handle_request_wrapper(server, stream));
    }
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
    info!("HTTP/1.1 {method} {path}", method=req.method.unwrap_or("GET"), path=req.path.unwrap_or("/"));
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
    let mut writer = io::BufWriter::new(stream);
    let buf = format!("HTTP/1.1 {code} {reason}",
        code=format!("{}", response.code.unwrap_or(200)),
        reason=response.reason.unwrap_or("OK"),
    );
    writer.write_all(&buf.as_bytes()).await?;
    for (name, value) in &response.headers {
        writer.write_all(NEWLINE).await?;
        writer.write_all(name.as_bytes()).await?;
        writer.write_all(b": ").await?;
        writer.write_all(&value).await?;
    }
    // one to end the last header/status line, and one as required by the protocol
    writer.write_all(NEWLINE).await?;
    writer.write_all(NEWLINE).await?;
    // Write the body
    if let Some(body) = response.body {
        writer.write_all(&body).await?;
    }
    writer.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use super::*;

    #[async_std::test]
    async fn test_hello_world() -> Result<(), Box<dyn Error>> {
        #[derive(Clone)]
        struct TestServer{}
    
        #[async_trait]
        impl Server for TestServer {
            async fn handle<'a>(&self, _request: &mut Request<'a>, response: &mut Response) -> io::Result<()> {
                response.headers.insert("Content-Type", b"text/html; charset=utf-8");
                response.body = Some(Vec::from("<h1>Hello world!</h1>"));
                Ok(())
            }
        }

        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let local_addr = listener.local_addr().unwrap();
        let _handle = task::spawn(async {
            serve(Arc::new(TestServer{}), listener).await;
        });
        // Make a simple HTTP request with some other library
        let path = format!("http://localhost:{}", local_addr.port());
        let res = ureq::get(&path).call();
        assert_eq!(res.status(), 200);
        assert_eq!(res.header("Content-Type").unwrap(), "text/html; charset=utf-8");
        assert_eq!(res.into_string().unwrap(), "<h1>Hello world!</h1>");
        Ok(())
    }

    #[async_std::test]
    async fn test_post_message() -> Result<(), Box<dyn Error>> {
        #[derive(Clone)]
        struct TestServer{}
    
        #[async_trait]
        impl Server for TestServer {
            async fn handle<'a>(&self, request: &mut Request<'a>, response: &mut Response) -> io::Result<()> {
                assert_eq!(request.method, "POST");
                response.headers.insert("Content-Type", b"text/html; charset=utf-8");
                Ok(())
            }
        }

        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let local_addr = listener.local_addr().unwrap();
        let _handle = task::spawn(async {
            serve(Arc::new(TestServer{}), listener).await;
        });
        // Make a simple HTTP request with some other library
        let path = format!("http://localhost:{}/post", local_addr.port());
        let params = [("foo", "bar"), ("baz", "quux")];
        let res = ureq::post(&path)
            .send_form(&params);
        assert_eq!(res.status(), 200);
        Ok(())
    }

    #[async_std::test]
    async fn test_shutdown_server() -> Result<(), Box<dyn Error>> {
        #[derive(Clone)]
        struct TestServer{}
    
        #[async_trait]
        impl Server for TestServer {
            async fn handle<'a>(&self, _: &mut Request<'a>, _: &mut Response) -> io::Result<()> {
                Ok(())
            }
        }

        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let listener2 = TcpListener::bind("127.0.0.1:0").await?;
        let local_addr = listener.local_addr().unwrap();
        
        // Spawn the server
        let my_server = HttpServer::new();
        let handle = my_server.spawn(Arc::new(TestServer{}), listener);
        let handle2 = my_server.spawn(Arc::new(TestServer{}), listener2);
        // Make a simple HTTP request with some other library, stop the server,and wait for join
        let path = format!("http://localhost:{}", local_addr.port());
        ureq::get(&path).call();
        ureq::get(&path).call();
        ureq::get(&path).call();
        my_server.shutdown();

        // Wait for the server to complate
        handle.await;
        handle2.await;

        // Verify we can re-listen on one of these things
        TcpListener::bind(local_addr).await?;
        Ok(())
    }
}