mod state;
mod error;

use axum::Router;
use socket2::{Domain, Protocol, Socket, Type};
use state::AppState;
use std::net::SocketAddr;
use tokio::net::TcpListener;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn bind(addr: SocketAddr, backlog: i32) -> std::io::Result<TcpListener> {
    let domain = match addr {
        SocketAddr::V4(_) => Domain::IPV4,
        SocketAddr::V6(_) => Domain::IPV6,
    };
    let socket = Socket::new(domain, Type::STREAM, Some(Protocol::TCP))?;
    socket.set_reuse_address(true)?;
    #[cfg(unix)]
    let _ = socket.set_reuse_port(true);
    socket.set_nonblocking(true)?;
    socket.bind(&addr.into())?;
    socket.listen(backlog)?;

    TcpListener::from_std(socket.into())
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr: SocketAddr = "0.0.0.0:3000".parse()?;
    let listener = bind(addr, 8192)?;

    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);

    println!("🌸 yume");
    println!("   addr     →  http://{addr}");
    println!("   threads  →  {threads}");

    let state = AppState {};
    let app = Router::new().with_state(state);
    axum::serve(listener, app).await?;

    Ok(())
}
