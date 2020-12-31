fn main() {
    println!("Hello world!");
}

/*
use std::io;
use std::error::Error;
use env_logger::Env;
use async_trait::async_trait;
use async_std::{
    prelude::*,
    sync::Arc,
    net::{
        TcpListener,
    },
};

#[async_std::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    let http_server = oc_http::HttpServer::new();
    let listener = TcpListener::bind("127.0.0.1:8080").await?;
    let server = Arc::new(MyServer{});
    let mut incoming = listener.incoming();
    while let Some(stream) = incoming.next().await {
        if let Ok(stream) = stream {
            http_server.dispatch(server.clone(), stream);
        }
    }
    Ok(())
}

struct MyServer {}

#[async_trait]
impl oc_http::Server for MyServer {
    async fn handle<'a>(&self, _request: &mut oc_http::Request<'a>, response: &mut oc_http::Response) -> io::Result<()> {
        response.body = Some(Vec::from("Hello world!"));
        Ok(())
    }
}*/