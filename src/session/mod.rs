use actix::{Actor, ActorContext, Addr, AsyncContext, Handler, Message, StreamHandler};
use actix_web_actors::ws::{self, ProtocolError, WebsocketContext};
use std::time::{Duration, Instant};

use crate::server::Server;

pub mod message;
use message::{IncomingMessage, OutgoingMessage};

/* @brief Client session responsible for keeping track of client identity,
 * handling client messages, etc
 */
pub struct Session {
    id: Option<String>,
    hb: Instant,
    server: Addr<Server>,
}

impl Session {
    pub fn new(server: Addr<Server>) -> Self {
        Self {
            id: None,
            hb: Instant::now(),
            server,
        }
    }
    /* @brief checks for ping every 5 seconds.
     * If the last ping was recorded earlier than 2 seconds ago, then the
     * client must have disconnected or have had some kind of network interruption
     */
    fn heartbeat(&mut self, ctx: &mut <Self as Actor>::Context) {
        ctx.run_interval(Duration::from_secs(5), |act, ctx| {
            if Instant::now().duration_since(act.hb).as_secs() >= 2 {
                ctx.stop();
            }
        });
    }
    fn handle_message(&mut self, msg: IncomingMessage, ctx: &mut <Self as Actor>::Context) {
        match msg {
            IncomingMessage::Login(id) => {
                if let Some(_) = &self.id {
                    log::error!("attempting to re-login");
                } else {
                    self.id = Some(id.clone());
                    self.server.do_send(crate::server::RegisterSession {
                        addr: ctx.address(),
                        id,
                    });
                }
            }
            _ => todo!("handle other messages"),
        }
    }
}

impl Actor for Session {
    type Context = WebsocketContext<Self>;
    fn started(&mut self, ctx: &mut Self::Context) {
        self.heartbeat(ctx);
    }
}

impl StreamHandler<Result<ws::Message, ProtocolError>> for Session {
    fn handle(&mut self, item: Result<ws::Message, ProtocolError>, ctx: &mut Self::Context) {
        match item {
            Ok(msg) => match msg {
                ws::Message::Text(text) => match serde_json::from_str::<IncomingMessage>(&text) {
                    Ok(msg) => self.handle_message(msg, ctx),

                    Err(err) => log::error!("Failed to deserialize message: {err}"),
                },
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
pub struct SerializedMessage(pub OutgoingMessage);

impl Handler<SerializedMessage> for Session {
    type Result = ();
    fn handle(&mut self, msg: SerializedMessage, ctx: &mut Self::Context) -> Self::Result {
        match serde_json::to_string(&msg.0) {
            Ok(str) => ctx.text(str),
            Err(err) => log::error!("error serializing message: {err}"),
        }
    }
}
