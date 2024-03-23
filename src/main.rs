mod game;
mod room;
mod server;
mod session;
mod utils;

#[actix::main]
async fn main() -> std::io::Result<()> {
    crate::server::http::start().await
}
