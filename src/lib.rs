use std::collections::HashMap;
use std::error::Error;

use httparse::{Response, Request};
use async_trait::async_trait;
use regex::Regex;
use log::{info, trace, warn};
use async_std::{
    prelude::*,
    io,
    net::TcpListener,
};

#[async_trait]
pub trait Endpoint {
    async fn call<'a>(&self, req: Request<'a, 'a>, resp: Response<'a, 'a>) -> Result<(), Box<dyn Error>>;
}

pub enum Route<F> {
    Router(Box<Router<F>>),
    Endpoint(F),
}

type Routes<F> = Vec<(Regex, Route<F>)>;

/// Router contains a handler and a set of routes, in order,
/// and react to the first match. It will either call the endpoint
/// or pass to the next router depending on the Match enum.
pub struct Router<F> {
    routes: Routes<F>,
}

impl<F> Router<F>
where F: Endpoint {
    pub fn new(routes: Routes<F>) -> Self {
        Router {
            routes,
        }
    }

    pub fn route(&self, request: Request) -> Result<(&Regex, &Route<F>), ()> {
        let path = match request.path {
            Some(path) => path,
            None => return Err(()),
        };
        for (regex, route) in &self.routes {
            if let Some(_) = regex.find(path) {
                match route {
                    Route::Endpoint(_) => return Ok((regex, route)),
                    Route::Router(rtr) => return rtr.route(request),
                }
            }
        }
        Err(())
    }
}

pub struct Server {
}

impl Server {
    pub fn new() -> Self {
        Server{}
    }

    pub async fn listen(listener: TcpListener) -> io::Result<()> {
        let mut incoming = listener.incoming();
        while let Some(stream) = incoming.next().await {
            match stream {
                Ok(stream) => {
                    info!("Accepting stream");
                },
                Err(err) => {
                    trace!("Error connecting to client; {:?}", err)
                },
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {

}