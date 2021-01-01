use std::{
    error::Error
};
use std::time::Duration;
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

use oc_http::{
    cookies::{Cookies, Cookie},
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

async fn handle_request<S>(socket: S)
where S: AsyncRead + AsyncWrite + Clone + Unpin
{
    // parse the http request; we'll make a /echo service;
    // first get a reader and writer buffer thing to improve
    // performance.
    let mut reader = BufReader::new(socket.clone());
    let mut writer = BufWriter::new(socket);
    // Read the response
    let request = match oc_http::http(&mut reader).await {
        Ok(req) => req,
        Err(err) => {
            warn!("Error {}", err);
            return;
        },
    };
    // get the cookie jar
    let mut cookies = Cookies::new(&request);
    // make sure it goes to /echo
    if request.path == "/echo" && request.method == "GET" {
        get_echo(&mut writer).await;
    } else if request.path == "/echo" && request.method == "POST" {
        post_echo(&mut reader, &mut writer).await;
        if let Some(_c) = cookies.get("Who") {
            writer.write(format!("You are a fool of a took!").as_bytes()).await.unwrap();
        }
    } else {
        let mut res = oc_http::Response{
            code: 404,
            reason: "NOT FOUND",
            headers: vec!(),
        };
        cookies.add_cookie(Cookie::new("Who", "You fool!"));
        cookies.write_cookies(&mut res);
        oc_http::respond(&mut writer, res).await.unwrap();
    }
    writer.flush().await.unwrap();
}

async fn get_echo<S>(mut stream: &mut S)
where S: AsyncWrite + Unpin
{
    oc_http::respond(&mut stream, oc_http::Response{
        code: 200,
        reason: "OK",
        headers: vec!(),
    }).await.unwrap();
    stream.write(b"
<html>
    <body>
        <form method=\"POST\">
            <input name=\"input\"></inpout>
            <input type=\"submit\"></input>
        </form>
    </body>
</html>
    ").await.unwrap();
}

async fn post_echo<W, R>(reader: &mut R, mut writer: &mut W)
where W: AsyncWrite + Unpin,
    R: AsyncRead + Unpin,
{
    oc_http::respond(&mut writer, oc_http::Response{
        code: 200,
        reason: "OK",
        headers: vec!(),
    }).await.unwrap();
    // read the body and see what the message is
    let mut buf = vec![0; 10];
    while let Ok(Ok(count)) = async_std::future::timeout(Duration::from_millis(10), reader.read(&mut buf)).await {
        if count == 0 {
            break;
        }
        writer.write_all(&buf[..count]).await.unwrap();
        writer.flush().await.unwrap();
    }
}