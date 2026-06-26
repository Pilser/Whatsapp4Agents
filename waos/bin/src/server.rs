use std::convert::Infallible;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use hyper::Server;
use hyper::service::{make_service_fn, service_fn};
use tokio::net::TcpSocket;

use crate::api;
use crate::daemon::DaemonState;

#[derive(Clone)]
pub struct ApiState {
    pub daemon: Arc<DaemonState>,
    pub frontend_path: Option<PathBuf>,
}

pub async fn run(
    port: u16,
    bind: &str,
    frontend_path: Option<PathBuf>,
    daemon: Arc<DaemonState>,
) -> anyhow::Result<()> {
    let addr: SocketAddr = format!("{bind}:{port}").parse()?;
    let socket = TcpSocket::new_v4()?;
    socket.set_nodelay(true)?;
    socket.set_reuseaddr(true)?;
    socket.bind(addr)?;
    let listener = socket.listen(1024)?;

    let state = Arc::new(ApiState { daemon, frontend_path });

    let make_svc = make_service_fn(move |_| {
        let state = state.clone();
        async move {
            Ok::<_, Infallible>(service_fn(move |req| {
                let state = state.clone();
                api::handle_request(req, state)
            }))
        }
    });

    let server = Server::from_tcp(listener.into_std()?)?
        .http1_keepalive(true)
        .tcp_nodelay(true)
        .serve(make_svc);

    server.await?;
    Ok(())
}
