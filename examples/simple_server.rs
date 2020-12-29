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
    oc_http::serve(Arc::new(Server{}), listener).await?;
    Ok(())
}

struct Server {}

#[async_trait]
impl oc_http::Server for Server {
    async fn handle<'a>(&self, _request: &mut oc_http::Request<'a>, response: &mut oc_http::Response) -> io::Result<()> {
        response.body = Some(Vec::from("Hello world!"));
        Ok(())
    }
}