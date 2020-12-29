use std::error::Error;
use async_std::sync::Arc;

use regex::Regex;
use httparse::{Request, Response};
use async_trait::async_trait;

use oc_http::{Endpoint, Router};

struct _State {}

type State = Arc<_State>;

fn main() -> Result<(), Box<dyn Error>> {
    let state = State::new(_State{});
    use oc_http::Route;
    let routes = vec!(
        (Regex::new("^/$")?, Route::Endpoint(Index{state})),
    );
    let router = Router::new(routes);
    Ok(())
}

struct Index {
    state: State,
}

#[async_trait]
impl Endpoint for Index {
    async fn call<'a>(&self, req: Request<'a, 'a>, resp: Response<'a, 'a>) -> Result<(), Box<dyn Error>> {
        Ok(())
    }
}