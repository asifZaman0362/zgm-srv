use actix_web::{
    web::{get, Payload},
    App, HttpRequest, HttpResponse, HttpServer,
};
use actix_web_actors::ws;

use crate::session::{message::SocketMessage, Session};

pub struct Http<T: SocketMessage> {
    _phantom: std::marker::PhantomData<T>,
}

impl<T: SocketMessage> Http<T> {
    async fn socket(req: HttpRequest, payload: Payload) -> actix_web::Result<HttpResponse> {
        ws::start(Session::<T>::new(), &req, payload)
    }
    pub async fn start() -> std::io::Result<()> {
        HttpServer::new(|| App::new().route("/ws", get().to(Self::socket)))
            .bind("0.0.0.0:8000")?
            .run()
            .await
    }
}
