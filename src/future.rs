use std::io;
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
use tokio::time::{ Sleep, Instant, sleep_until };
use tower_service::Service;
use pin_project_lite::pin_project;


pin_project!{
    pub struct HappyEyeballsFut<MC: Service<IpAddr>, IP> {
        dur: Duration,
        #[pin]
        timer: Sleep,
        want: bool,

        make_conn: MC,
        ips: Sort<IP>,
        queue: FuturesUnordered<MC::Future>,
    }
}

struct Sort<IP> {
    queue: VecDeque<IpAddr>,
    ipflag: Option<bool>,
    ips: IP
}

impl<MC: Service<IpAddr>, IP> HappyEyeballsFut<MC, IP> {
    #[inline]
    pub(crate) fn new(dur: Duration, make_conn: MC, ips: IP) -> Self {
        HappyEyeballsFut {
            dur, make_conn,
            want: true,
            ips: Sort { queue: VecDeque::new(), ipflag: None, ips },
            queue: FuturesUnordered::new(),
            timer: sleep_until(Instant::now())
        }
    }
}

impl<MC, IP> Future for HappyEyeballsFut<MC, IP>
where
    MC: Service<IpAddr> + Unpin,
    MC::Error: From<io::Error>,
    IP: Stream<Item = IpAddr> + FusedStream + Unpin
{
    type Output = Result<MC::Response, MC::Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let mut this = self.project();

        *this.want |= this.timer.as_mut().poll(cx).is_ready();

        if *this.want && this.make_conn.poll_ready(cx).is_ready() {
            if let Poll::Ready(Some(ip)) = Pin::new(&mut this.ips).poll_next(cx) {
                let fut = this.make_conn.call(ip);
                this.queue.push(fut);

                let dst = Instant::now() + *this.dur;
                this.timer.as_mut().reset(dst);
                let _ = this.timer.as_mut().poll(cx);
                *this.want = false;
            }
        }

        match Pin::new(&mut this.queue).poll_next(cx) {
            Poll::Ready(Some(Ok(output))) => Poll::Ready(Ok(output)),
            Poll::Ready(Some(Err(err))) if this.queue.is_empty() && this.ips.is_terminated() =>
                Poll::Ready(Err(err)),
            Poll::Ready(None) if this.queue.is_empty() && this.ips.is_terminated() =>
                Poll::Ready(Err(empty_err().into())),
            Poll::Ready(Some(Err(_))) | Poll::Ready(None) => {
                *this.want = true;
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

#[cold]
fn empty_err() -> io::Error {
    io::Error::new(
        io::ErrorKind::NotFound,
        "happy eyeballs not found any address"
    )
}
