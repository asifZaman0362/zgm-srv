mod game;
mod room;
mod server;
mod session;
mod utils;
mod session_manager;

#[actix::main]
async fn main() -> std::io::Result<()> {
    crate::server::http::start().await
}
