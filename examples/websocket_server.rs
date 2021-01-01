use std::{
    error::Error
};
use log::{warn};

use env_logger::Env;
use async_std::{
    task,
    io::{
        BufReader,
        BufWriter,
    },
    net::{
        TcpListener,
    },
};

use futures::{
    prelude::*,
    AsyncRead,
    AsyncWrite,
};

use oc_http::websocket::{
    self,
    WebSocketReader,
    WebSocketWriter,
};

#[async_std::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    // start the server; we could reduce this to one line, but then you have to write an entire struct
    // to support it (or learn how to use super-dense map reduces)
    let listener = TcpListener::bind("127.0.0.1:8080").await?;
    let _local_addr = listener.local_addr()?;
    let mut incoming = listener.incoming();
    // Accepting incoming reqeusts
    while let Some(stream) = incoming.next().await {
        // consider that I have 12 cores; so a single thread would need to run at 1/12 of my total CPU to
        // block other threads; therefore, this thread could do 1/12 of what a single thread does in order
        // to become the bottleneck.. Which is a fair bit, so don't be stingy.
        if let Ok(stream) = stream {
            task::spawn(handle_request(stream));
        }
    }
    Ok(())
}

async fn handle_request<S>(stream: S)
where S: AsyncRead + AsyncWrite + Clone + Unpin
{
    // parse the http request; we'll make a /echo service;
    // first get a reader and writer buffer thing to improve
    // performance.
    let mut reader = BufReader::new(stream.clone());
    // Read the response
    let request = match oc_http::http(&mut reader).await {
        Ok(req) => req,
        Err(err) => {
            warn!("Error {}", err);
            return;
        },
    };
    // make sure it goes to /ws
    if request.path == "/ws" && request.method == "GET" {
        let ws = websocket::upgrade(&request, stream).await.unwrap();
        handle_websocket(ws.0, ws.1).await;
    } else {
        let mut writer = BufWriter::new(stream);
        oc_http::respond(&mut writer, oc_http::Response{
            code: 404,
            reason: "NOT FOUND",
            headers: vec!(),
        }).await.unwrap();
        writer.flush().await.unwrap();
    }
}

async fn handle_websocket<S>(mut rdr: WebSocketReader<S>, mut wrt: WebSocketWriter<S>)
where S: AsyncRead + AsyncWrite + Clone + Unpin {
    loop {
         let msg = rdr.recv().await.unwrap();
         wrt.write(&msg).await.unwrap();
    }
}