mod game;
mod room;
mod server;
mod session;

#[actix::main]
async fn main() -> std::io::Result<()> {
    crate::server::http::start().await
}
