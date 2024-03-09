use actix::{Actor, ActorContext, AsyncContext, Handler, Message, StreamHandler, Addr};
use actix_web_actors::ws::{self, ProtocolError, WebsocketContext};
use serde::Serialize;
use std::time::{Duration, Instant};

use crate::server::Server;

pub mod message;
use message::{IncomingMessage, OutgoingMessage, SocketMessage};

pub struct Session {
    id: Option<String>,
    hb: Instant,
    server: Addr<Server<M>>
}

impl<M: SocketMessage> Session<M> {
    pub fn new(server: Addr<Server<M>>) -> Self {
        Self {
            id: None,
            hb: Instant::now(),
            server
        }
    }
    fn heartbeat(&mut self, ctx: &mut <Self as Actor>::Context) {
        ctx.run_interval(Duration::from_secs(5), |act, ctx| {
            if Instant::now().duration_since(act.hb).as_secs() >= 2 {
                ctx.stop();
            }
        });
    }
    fn handle_message(&mut self, msg: IncomingMessage<M>) {
        match msg {
            IncomingMessage::Login(id) => {
                if let Some(_) = &self.id {
                    log::error!("attempting to re-login");
                } else {
                    self.id = Some(id);
                    todo!("register on server")
                }
            }
            IncomingMessage::Message(_inner) => {
                todo!("handle")
            }
        }
    }
}

impl<M: SocketMessage> Actor for Session<M> {
    type Context = WebsocketContext<Self>;
    fn started(&mut self, ctx: &mut Self::Context) {
        self.heartbeat(ctx);
    }
}

impl<M: SocketMessage> StreamHandler<Result<ws::Message, ProtocolError>> for Session<M> {
    fn handle(&mut self, item: Result<ws::Message, ProtocolError>, ctx: &mut Self::Context) {
        match item {
            Ok(msg) => match msg {
                ws::Message::Text(text) => {
                    match serde_json::from_str::<IncomingMessage<M>>(&text) {
                        Ok(msg) => self.handle_message(msg),

                        Err(err) => log::error!("Failed to deserialize message: {err}"),
                    }
                }
                ws::Message::Ping(bytes) => ctx.pong(&bytes),
                ws::Message::Close(reason) => ctx.close(reason),
                _ => {}
            },
            Err(err) => log::error!("{err}"),
        }
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct SerializedMessage<T: Serialize>(OutgoingMessage<T>);

impl<M: SocketMessage, T: Serialize> Handler<SerializedMessage<T>> for Session<M> {
    type Result = ();
    fn handle(&mut self, msg: SerializedMessage<T>, ctx: &mut Self::Context) -> Self::Result {
        match serde_json::to_string(&msg.0) {
            Ok(str) => ctx.text(str),
            Err(err) => log::error!("error serializing message: {err}"),
        }
    }
}
