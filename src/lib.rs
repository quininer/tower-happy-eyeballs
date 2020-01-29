pub mod future;

use std::net::IpAddr;
use std::marker::Unpin;
use std::time::Duration;
use std::task::{ Context, Poll };
use futures_core::stream::{ Stream, FusedStream };
use tower_service::Service;
use tower_layer::Layer;
use future::HappyEyeballsFut;


#[derive(Clone, Debug)]
pub struct HappyEyeballsLayer<E> {
    err_fn: fn() -> E,
    dur: Duration
}

pub struct HappyEyeballs<MC: Service<IpAddr>> {
    err_fn: fn() -> MC::Error,
    dur: Duration,
    make_conn: MC
}

impl<E> HappyEyeballsLayer<E> {
    pub fn new(err_fn: fn() -> E) -> HappyEyeballsLayer<E> {
        HappyEyeballsLayer { err_fn, dur: Duration::from_millis(250) }
    }

    pub fn delay(mut self, dur: Duration) -> Self {
        self.dur = dur;
        self
    }
}

impl<MC> Layer<MC> for HappyEyeballsLayer<MC::Error>
where
    MC: Service<IpAddr>
{
    type Service = HappyEyeballs<MC>;

    fn layer(&self, service: MC) -> Self::Service {
        HappyEyeballs {
            make_conn: service,
            err_fn: self.err_fn,
            dur: self.dur.clone()
        }
    }
}

impl<IP, MC> Service<IP> for HappyEyeballs<MC>
where
    MC: Service<IpAddr> + Unpin + Clone,
    IP: Stream<Item = IpAddr> + FusedStream + Unpin
{
    type Response = MC::Response;
    type Error = MC::Error;
    type Future = HappyEyeballsFut<MC, IP>;

    fn poll_ready(&mut self, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        self.make_conn.poll_ready(cx)
    }

    fn call(&mut self, req: IP) -> Self::Future {
        HappyEyeballsFut::new(self.err_fn, self.dur.clone(), self.make_conn.clone(), req)
    }
}
