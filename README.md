# Overcodes HTTP Server

This is a super simple HTTP server that doesn't get in the way.

That is all.

## Getting started

Add to Cargo.toml

```
oc_http = "0.1.0"
```

Optionally, I recommend

```

async-std = {version = "1.8.0", features = ["attributes"]}
log = "0.4"
async-trait = "0.1"
env_logger = "0.8"
```

Create a server:

```
struct Server {}

#[async_trait]
impl oc_http::Server for Server {
    async fn handle<'a>(&self, _request: &mut oc_http::Request<'a>, response: &mut oc_http::Response) -> io::Result<()> {
        response.body = Some(Vec::from("Hello world!"));
        Ok(())
    }
}
```

And hook that serverup to a TCP socket (if you used the suggested packages above, copy pasta):

```
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
    // by default, log at the info level, but override with environment variable RUST_LOG
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    // Construct a TCP socket
    let listener = TcpListener::bind("127.0.0.1:8080").await?;
    // Server HTTP stuff
    oc_http::serve(Arc::new(Server{}), listener).await?;
    Ok(())
}
```

## Testing

Let's say you want to test your server. This is how I do it:

```
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
```

The pro to using oc_http::HttpServer here rather than just oc_http::serve is that cleanup is done when the test exits;
we won't hold onto a bunch of garbage TCP sockets. This could also be useful if you want to do a proper cleanup when
your process exits, or if you need to 'restart' the http server.