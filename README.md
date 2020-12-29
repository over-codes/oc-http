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
    use async_std::{
        task,
        net::SocketAddr,
    }

    fn run_server() -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let local_addr = listener.local_addr().unwrap();
        task::spawn(serve(Arc::new(MyServer{}), listener).await.unwrap());
        local_addr
    }

    #[async_std::test]
    async fn test_my_server() {
        let remote = run_server();
        let path = format!("http://localhost:{}", remote.port());
        // do things
    }
}
```