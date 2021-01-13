use std::{
    collections::HashMap,
    str,
};

use log::{warn};
pub use cookie::Cookie;

use crate::{
    Request,
    Response,
};

pub struct Cookies<'c> {
    cookies: HashMap<String, Cookie<'c>>,
    cookies_to_set: Vec<Cookie<'c>>,
}

impl<'a> Cookies<'a> {
    pub fn new(req: &'a Request) -> Self {
        let mut cookies = HashMap::default();
        let iter_cookies = req.headers.get("Cookie");
        if iter_cookies.is_none() {
            return Cookies{
                cookies,
                cookies_to_set: vec!(),
            }
        }
        for cookie in iter_cookies.unwrap().0.split(|x| *x == b';') {
            let cookie = match str::from_utf8(cookie) {
                Ok(s) => s,
                Err(_) => {
                    warn!("Invalid cookie being ignored!");
                    continue;
                }
            };
            let cookie = Cookie::parse_encoded(cookie);
            if cookie.is_err() {
                warn!("Invalid cookie being ignored!");
                continue;
            }
            let cookie = cookie.unwrap();
            cookies.insert(String::from(cookie.name()), cookie);
        }
        Cookies {
            cookies,
            cookies_to_set: vec!(),
        }
    }

    pub fn get(&self, s: &str) -> Option<&'a Cookie> {
        self.cookies.get(s)
    }

    pub fn add_cookie(&mut self, cookie: Cookie<'a>) {
        self.cookies_to_set.push(cookie.clone());
        self.cookies.insert(String::from(cookie.name()), cookie);
    }

    pub fn write_cookies(&self, resp: &mut Response) {
        for cookie in &self.cookies_to_set {
            resp.headers.push(("Set-Cookie".into(), Vec::from(format!("{}", cookie.encoded()))));
        }
    }
}