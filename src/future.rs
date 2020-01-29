use std::pin::Pin;
use std::net::IpAddr;
use std::marker::Unpin;
use std::time::Duration;
use std::future::Future;
use std::collections::VecDeque;
use std::task::{ Context, Poll };
use futures_core::ready;
use futures_core::stream::{ Stream, FusedStream };
use futures_util::stream::FuturesUnordered;
use tokio::time::{ Delay, Instant, delay_until };
use tower_service::Service;


pub struct HappyEyeballsFut<MC: Service<IpAddr>, IP> {
    err_fn: fn() -> MC::Error,
    dur: Duration,
    timer: Delay,
    want: bool,

    make_conn: MC,
    ips: Sort<IP>,
    queue: FuturesUnordered<MC::Future>,
}

struct Sort<IP> {
    queue: VecDeque<IpAddr>,
    ipflag: Option<bool>,
    ips: IP
}

impl<MC: Service<IpAddr>, IP> HappyEyeballsFut<MC, IP> {
    #[inline]
    pub(crate) fn new(err_fn: fn() -> MC::Error, dur: Duration, make_conn: MC, ips: IP) -> Self {
        HappyEyeballsFut {
            err_fn, dur, make_conn,
            want: true,
            ips: Sort { queue: VecDeque::new(), ipflag: None, ips },
            queue: FuturesUnordered::new(),
            timer: delay_until(Instant::now())
        }
    }
}

impl<MC, IP> Future for HappyEyeballsFut<MC, IP>
where
    MC: Service<IpAddr> + Unpin,
    IP: Stream<Item = IpAddr> + FusedStream + Unpin
{
    type Output = Result<MC::Response, MC::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        self.want |= Pin::new(&mut self.timer).poll(cx).is_ready();

        if self.want && self.make_conn.poll_ready(cx).is_ready() {
            if let Poll::Ready(Some(ip)) = Pin::new(&mut self.ips).poll_next(cx) {
                let fut = self.make_conn.call(ip);
                self.queue.push(fut);

                let dst = Instant::now() + self.dur;
                self.timer.reset(dst);
                let _ = Pin::new(&mut self.timer).poll(cx);
                self.want = false;
            }
        }

        match Pin::new(&mut self.queue).poll_next(cx) {
            Poll::Ready(Some(Ok(output))) => Poll::Ready(Ok(output)),
            Poll::Ready(Some(Err(err))) if self.queue.is_empty() && self.ips.is_terminated() => Poll::Ready(Err(err)),
            Poll::Ready(None) if self.queue.is_empty() && self.ips.is_terminated() => Poll::Ready(Err((self.err_fn)())),
            Poll::Ready(Some(Err(_))) | Poll::Ready(None) => {
                self.want = true;
                cx.waker().wake_by_ref();
                Poll::Pending
            },
            Poll::Pending => Poll::Pending
        }
    }
}

impl<IP> Stream for Sort<IP>
where
    IP: Stream<Item = IpAddr> + Unpin
{
    type Item = IP::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();

        if let Some(ipflag) = this.ipflag.as_mut() {
            if let Some(ip) = this.queue.front() {
                if ip.is_ipv6() != *ipflag {
                    *ipflag = ip.is_ipv6();
                    return Poll::Ready(this.queue.pop_front());
                }
            }

            let ip = ready!(Pin::new(&mut this.ips).poll_next(cx));

            match ip {
                Some(ip) if ip.is_ipv6() != *ipflag => {
                    *ipflag = ip.is_ipv6();
                    Poll::Ready(Some(ip))
                },
                Some(ip) => {
                    this.queue.push_back(ip);
                    Pin::new(this).poll_next(cx)
                }
                None => Poll::Ready(this.queue.pop_front())
            }
        } else {
            let ip = ready!(Pin::new(&mut this.ips).poll_next(cx));
            this.ipflag = ip.as_ref().map(|ip| ip.is_ipv6());
            Poll::Ready(ip)
        }
    }
}

impl<IP> FusedStream for Sort<IP>
where
    IP: Stream<Item = IpAddr> + Unpin,
    IP: FusedStream
{
    fn is_terminated(&self) -> bool {
        self.queue.is_empty() && self.ips.is_terminated()
    }
}
