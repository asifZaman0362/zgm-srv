use actix::{Actor, Addr};
use actix_web::{
    web::{get, Data, Payload},
    App, HttpRequest, HttpResponse, HttpServer,
};
use actix_web_actors::ws;

use crate::session::{SessionManager, actor::Session};
use crate::room::RoomManager;

async fn socket(
    req: HttpRequest,
    payload: Payload,
    data: Data<(Addr<SessionManager>, Addr<RoomManager>)>,
) -> actix_web::Result<HttpResponse> {
    let (session_manager, room_manager) = data.get_ref();
    ws::start(Session::new(session_manager.to_owned(), room_manager.to_owned()), &req, payload)
}
pub async fn start() -> std::io::Result<()> {
    let session_manager = SessionManager::new().start();
    let room_manager = RoomManager::new().start();
    HttpServer::new(move || {
        App::new()
            .route("/ws", get().to(socket))
            .app_data(Data::new((session_manager.clone(), room_manager.clone())))
    })
    .bind("0.0.0.0:8000")?
    .run()
    .await
}
