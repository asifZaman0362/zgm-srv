mod server;
mod session;
mod room;
use serde::Deserialize;

#[derive(Deserialize)]
struct Message;

impl crate::session::message::SocketMessage for Message {}

#[actix::main]
async fn main() -> std::io::Result<()> {
    crate::server::http::Http::<Message>::start().await
}
