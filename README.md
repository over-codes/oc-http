# Overcodes HTTP Server

This is a super simple HTTP server that doesn't get in the way.

It doesn't do much, but it doesn't hide much. Additional features are exposed through various
modules, including:

- Setting/getting cookies
- Websockets

This library is async, but does not dictate whether you use tokio, async-std, or something else.

My goal is to write a HTTP library that isn't confusing, doesn't prevent anything (even if it may
require a few extra lines to get my goal), and has reasonable performance.

## Getting started

If you're up for it, look at [examples/simple_server.rs], [examples/websocket_server.rs], and [examples/echo_server.rs]. They'll be up-to-date, unlike this doc.

[examples/echo_server.rs]: https://github.com/over-codes/oc-http/blob/main/examples/echo_server.rs
[examples/websocket_server.rs]: https://github.com/over-codes/oc-http/blob/main/examples/websocket_server.rs
[examples/simple_server.rs]: https://github.com/over-codes/oc-http/blob/main/examples/simple_server.rs

Add to Cargo.toml

```
oc_http = "0.1.0"
```

I use async-std because it's easy, plus logging stuff;

```
async-std = {version = "1.8.0", features = ["attributes"]}
log = "0.4"
env_logger = "0.8"
```

Create a server:

```
use std::error::Error;
use log::warn;
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
    // setup the logger
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
```