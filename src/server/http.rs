use super::Server;
use actix::{Actor, Addr};
use actix_web::{
    web::{get, Data, Payload},
    App, HttpRequest, HttpResponse, HttpServer,
};
use actix_web_actors::ws;
use std::sync::{Arc, Mutex};

use crate::session::Session;

async fn socket(
    req: HttpRequest,
    payload: Payload,
    data: Data<(Addr<Server>, Arc<Mutex<u128>>)>,
) -> actix_web::Result<HttpResponse> {
    let data = data.get_ref();
    match data.1.lock() {
        Ok(mut id) => {
            *id += 1;
            ws::start(Session::new(data.0.to_owned(), *id), &req, payload)
        }
        Err(err) => {
            log::error!("{err}");
            Err(actix_web::error::ErrorInternalServerError("server error"))
        }
    }
}
pub async fn start() -> std::io::Result<()> {
    let id_provider = Arc::new(Mutex::new(0u128));
    let server = Server::new().start();
    HttpServer::new(move || {
        App::new()
            .route("/ws", get().to(socket))
            .app_data(Data::new((server.clone(), Arc::clone(&id_provider))))
    })
    .bind("0.0.0.0:8000")?
    .run()
    .await
}
