use std::io;
use std::error::Error;
use env_logger::Env;
use async_trait::async_trait;
use async_std::{
    sync::Arc,
    net::{
        TcpListener,
    },
};

#[async_std::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    let listener = TcpListener::bind("127.0.0.1:8080").await?;
    oc_http::serve(Arc::new(MyServer{}), listener).await;
    Ok(())
}

struct MyServer {}

#[async_trait]
impl oc_http::Server for MyServer {
    async fn handle<'a>(&self, _request: &mut oc_http::Request<'a>, response: &mut oc_http::Response) -> io::Result<()> {
        response.body = Some(Vec::from("Hello world!"));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oc_http::HttpServer;

    async fn run_server() -> (String, HttpServer) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let local_addr = listener.local_addr().unwrap();
        let http_server = HttpServer::new();
        http_server.spawn(Arc::new(MyServer{}), listener);
        (format!("http://localhost:{}", local_addr.port()), http_server)
    }

    #[async_std::test]
    async fn test_my_server() {
        // when goes out of scope, the socket will close; this simplifies running many tests as we are
        // less likely to run out of ports or break in some funny way
        let (base_path, _http_server) = run_server().await;
        // do things
        let resp = ureq::get(&format!("{}{}", base_path, "/hello")).call();
        assert_eq!(resp.status(), 200);
        assert_eq!(resp.into_string().unwrap(), "Hello world!");
    }
}