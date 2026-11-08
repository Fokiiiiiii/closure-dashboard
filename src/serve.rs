//! The server the dashboard runs on: what is limited before a request is even read.
//!
//! `axum::serve` waits as long as a client likes for request headers, so idle connections could hold it.
//! Here connections are capped, an extra one is closed at once, and a slow header is cut off by a timeout.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::{TokioIo, TokioTimer};
use tokio::net::TcpListener;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tower::Service;

/// Connections served at once.
const MAX_CONNECTIONS: usize = 512;
/// How long a client has to send the headers of a request.
const HEADER_TIMEOUT: Duration = Duration::from_secs(10);
/// How long the connections under way are waited for once the server is told to stop.
const DRAIN: Duration = Duration::from_secs(5);

/// Serves `router` on `listener` until `shutdown` is done, then gives the requests under way a
/// few seconds to finish.
pub async fn serve(listener: TcpListener, router: Router, shutdown: impl Future<Output = ()>) {
    let permits = Arc::new(Semaphore::new(MAX_CONNECTIONS));
    let mut connections = JoinSet::new();
    let mut protocol = http1::Builder::new();
    protocol
        .timer(TokioTimer::new())
        .header_read_timeout(HEADER_TIMEOUT);
    tokio::pin!(shutdown);
    loop {
        let accepted = tokio::select! {
            biased;
            () = &mut shutdown => break,
            accepted = listener.accept() => accepted,
        };
        let stream = match accepted {
            Ok((stream, _)) => stream,
            Err(error) => {
                // Out of file descriptors and the like: do not spin on it.
                tracing::warn!(%error, "could not accept a connection");
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
        };
        let Ok(permit) = Arc::clone(&permits).try_acquire_owned() else {
            drop(stream);
            continue;
        };
        let router = router.clone();
        let protocol = protocol.clone();
        connections.spawn(async move {
            let _permit = permit;
            let service =
                service_fn(move |request: hyper::Request<Incoming>| router.clone().call(request));
            if let Err(error) = protocol
                .serve_connection(TokioIo::new(stream), service)
                .await
            {
                // Clients that leave or are too slow are routine.
                tracing::debug!(%error, "connection ended with an error");
            }
        });
        // Finished connections are collected as the next one comes.
        while connections.try_join_next().is_some() {}
    }
    let _ = tokio::time::timeout(DRAIN, async {
        while connections.join_next().await.is_some() {}
    })
    .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::routing::get;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;
    use tokio::sync::oneshot;

    #[tokio::test]
    async fn answers_a_request_and_stops_on_shutdown() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let address = listener.local_addr().expect("address");
        let (stop, stopped) = oneshot::channel::<()>();
        let server = tokio::spawn(serve(
            listener,
            Router::new().route("/", get(|| async { "ok" })),
            async {
                let _ = stopped.await;
            },
        ));

        let mut client = TcpStream::connect(address).await.expect("connect");
        client
            .write_all(b"GET / HTTP/1.1\r\nhost: x\r\nconnection: close\r\n\r\n")
            .await
            .expect("write");
        let mut reply = String::new();
        client.read_to_string(&mut reply).await.expect("read");
        assert!(reply.starts_with("HTTP/1.1 200"), "{reply}");

        stop.send(()).expect("stop");
        server.await.expect("server task");
    }

    #[tokio::test(start_paused = true)]
    async fn cuts_off_a_client_that_never_finishes_its_headers() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let address = listener.local_addr().expect("address");
        let (stop, stopped) = oneshot::channel::<()>();
        let server = tokio::spawn(serve(listener, Router::new(), async {
            let _ = stopped.await;
        }));

        let mut client = TcpStream::connect(address).await.expect("connect");
        client.write_all(b"GET / HT").await.expect("write");
        let mut rest = Vec::new();
        // The server closes the connection once HEADER_TIMEOUT has passed (time is paused, so
        // this does not take ten real seconds).
        client.read_to_end(&mut rest).await.expect("read");

        stop.send(()).expect("stop");
        server.await.expect("server task");
    }
}
