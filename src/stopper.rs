use async_std::{
    io,
    net::{
        TcpStream,
    },
    channel::{
        Sender,
        Receiver,
        bounded,
    },
};

#[derive(Clone)]
pub struct StopToken {
    done: Receiver<u8>,
}

impl StopToken {
    pub async fn wait(&self) -> Option<io::Result<TcpStream>> {
        while let Ok(_) = self.done.recv().await {
            // loop until we get an error about the sender being closed
        }
        None
    }
}

pub struct Stopper {
    done: Sender<u8>,
}

impl Stopper {
    pub fn new() -> (Self, StopToken) {
        let (s, r) = bounded(1);
        (Stopper{
            done: s,
        }, StopToken{
            done: r,
        })
    }

    pub fn shutdown(&self) {
        self.done.close();
    }
}