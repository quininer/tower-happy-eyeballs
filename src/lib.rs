pub mod future;

use std::io;
use std::net::IpAddr;
use std::marker::Unpin;
use std::time::Duration;
use std::task::{ Context, Poll };
use futures_core::stream::{ Stream, FusedStream };
use tower_service::Service;
use tower_layer::Layer;
use future::HappyEyeballsFut;


#[derive(Clone, Debug)]
pub struct HappyEyeballsLayer {
    dur: Duration
}

pub struct HappyEyeballs<MC>
where
    MC: Service<IpAddr>,
    MC::Error: From<io::Error>
{
    dur: Duration,
    make_conn: MC
}

impl HappyEyeballsLayer {
    pub fn new() -> HappyEyeballsLayer {
        HappyEyeballsLayer { dur: Duration::from_millis(250) }
    }

    pub fn delay(mut self, dur: Duration) -> Self {
        self.dur = dur;
        self
    }
}

impl<MC> Layer<MC> for HappyEyeballsLayer
where
    MC: Service<IpAddr>,
    MC::Error: From<io::Error>
{
    type Service = HappyEyeballs<MC>;

    fn layer(&self, service: MC) -> Self::Service {
        HappyEyeballs {
            make_conn: service,
            dur: self.dur.clone()
        }
    }
}

impl<IP, MC> Service<IP> for HappyEyeballs<MC>
where
    MC: Service<IpAddr> + Unpin + Clone,
    MC::Error: From<io::Error>,
    IP: Stream<Item = IpAddr> + FusedStream + Unpin
{
    type Response = MC::Response;
    type Error = MC::Error;
    type Future = HappyEyeballsFut<MC, IP>;

    fn poll_ready(&mut self, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        self.make_conn.poll_ready(cx)
    }

    fn call(&mut self, req: IP) -> Self::Future {
        HappyEyeballsFut::new(self.dur.clone(), self.make_conn.clone(), req)
    }
}
