use super::Server;
use actix::Addr;
use actix_web::{
    web::{get, Data, Payload},
    App, HttpRequest, HttpResponse, HttpServer,
};
use actix_web_actors::ws;

use crate::session::Session;

async fn socket(
    req: HttpRequest,
    payload: Payload,
    data: Data<Addr<Server>>,
) -> actix_web::Result<HttpResponse> {
    ws::start(Session::new(data.get_ref().to_owned()), &req, payload)
}
pub async fn start() -> std::io::Result<()> {
    HttpServer::new(|| App::new().route("/ws", get().to(socket)))
        .bind("0.0.0.0:8000")?
        .run()
        .await
}
