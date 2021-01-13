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
    WebSocketError,
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
    let mut buf = vec![0; 65536];
    let request = match oc_http::http(&mut reader, &mut buf).await {
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
        // this will return an error when the socket is closed;
        // oc_http::websocket::WebSocketError::ConnectionClosed
         let msg = match rdr.recv().await {
             Ok(msg) => msg,
             Err(WebSocketError::ConnectionClosed) => return,
             Err(err) => {
                 warn!("Sadness is {:?}", err);
                 return
             },
         };
         wrt.write(&msg).await.unwrap();
    }
}