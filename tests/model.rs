use std::io;
use std::task::Poll;
use std::net::IpAddr;
use std::time::Instant;
use futures_util::future;
use futures_util::stream::{ self, StreamExt };
use tower_layer::Layer;
use tower_util::{ service_fn, ServiceExt };
use tower_happy_eyeballs::HappyEyeballsLayer;


struct Connect(IpAddr);

#[tokio::test]
async fn test_happy_eyeballs() -> io::Result<()> {
    let ips = vec![
        IpAddr::from([1, 0, 0, 0, 0, 0, 0, 1]),
        IpAddr::from([1, 0, 0, 0, 0, 0, 0, 2]),
        IpAddr::from([1, 0, 0, 0, 0, 0, 0, 3]),
        IpAddr::from([1, 0, 0, 1]),
        IpAddr::from([1, 0, 0, 2]),
        IpAddr::from([1, 0, 0, 3]),
    ];

    let make_conn = service_fn(|ip| future::poll_fn(move |_| -> Poll<io::Result<Connect>> {
        if ip == IpAddr::from([1, 0, 0, 3]) {
            Poll::Ready(Ok(Connect(ip)))
        } else if ip == IpAddr::from([1, 0, 0, 0, 0, 0, 0, 2]) {
            Poll::Ready(Err(io::ErrorKind::ConnectionReset.into()))
        } else {
            Poll::Pending
        }
    }));

    let now = Instant::now();

    let Connect(ip) = HappyEyeballsLayer::new(|| io::ErrorKind::NotFound.into())
        .layer(make_conn)
        .oneshot(stream::iter(ips).fuse()).await?;

    assert_eq!(now.elapsed().as_secs(), 1);
    assert_eq!(ip, IpAddr::from([1, 0, 0, 3]));

    Ok(())
}
