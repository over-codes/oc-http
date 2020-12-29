use std::io;
use std::error::Error;
use async_std::sync::Arc;



use async_trait::async_trait;
use async_std::{
    net::{
        TcpListener,
    },
};

#[async_std::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();
    let listener = TcpListener::bind("127.0.0.1:8080").await?;
    oc_http::serve(Arc::new(Server{}), listener).await?;
    Ok(())
}

#[derive(Clone)]
struct Server {}

#[async_trait]
impl oc_http::Server for Server {
    async fn handle<'a>(&self, _request: &mut oc_http::Request<'a>, response: &mut oc_http::Response) -> io::Result<()> {
        response.body = Some("Hello world!".to_string().as_bytes().to_vec());
        Ok(())
    }
}