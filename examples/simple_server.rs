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
    net::TcpListener,
};

use futures::{
    prelude::*,
    AsyncRead,
    AsyncWrite,
};

#[async_std::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    // start the server; this uses standard stdlib-esque tools rather than saving a few
    // lines by just sending the ToSocketAddr item.
    let listener = TcpListener::bind("127.0.0.1:8080").await?;
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
    // parse the http request; prefer using BufWriter/BufReader for performance.
    let mut reader = BufReader::new(stream.clone());
    let mut writer = BufWriter::new(stream);
    // Read the request
    match oc_http::http(&mut reader).await {
        Ok(req) => req,
        Err(err) => {
            warn!("Error {}", err);
            return;
        },
    };
    oc_http::respond(&mut writer, oc_http::Response{
        code: 200,
        reason: "OK",
        headers: vec!(),
    }).await.unwrap();
    // after sending the HTTP header, we can write anything to the body
    writer.write(b"
<html>
    <body>
        <h1>Hello world!</h1>
    </body>
</html>
    ").await.unwrap();
    writer.flush().await.unwrap();
}