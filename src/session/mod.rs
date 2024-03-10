use actix::{
    Actor, ActorContext, Addr, AsyncContext, Handler, Message, SpawnHandle, StreamHandler,
};
use actix_web_actors::ws::{self, ProtocolError, WebsocketContext};
use std::time::{Duration, Instant};

use crate::server::{DeregisterSession, RegisterSession, Server};

pub mod message;
use crate::room::Room;
use crate::session::message::RemoveReason;
use message::{IncomingMessage, OutgoingMessage};

const RECONNECTION_TIME_LIMIT: u64 = 15;
const HB_CHECK_INTERVAL: u64 = 5;
const HB_TIMEOUT: u64 = 2;

/* @brief Client session responsible for keeping track of client identity,
 * handling client messages, etc
 */
pub struct Session {
    id: Option<String>,
    server_id: u128,
    hb: Instant,
    server: Addr<Server>,
    reconnection_timer: Option<SpawnHandle>,
    room: Option<Addr<Room>>,
}

impl Session {
    pub fn new(server: Addr<Server>, server_id: u128) -> Self {
        Self {
            id: None,
            hb: Instant::now(),
            server,
            server_id,
            reconnection_timer: None,
            room: None,
        }
    }
    /* @brief checks for ping every 5 seconds.
     * If the last ping was recorded earlier than 2 seconds ago, then the
     * client must have disconnected or have had some kind of network interruption
     */
    fn heartbeat(&mut self, ctx: &mut <Self as Actor>::Context) {
        ctx.run_interval(Duration::from_secs(HB_CHECK_INTERVAL), |act, ctx| {
            if Instant::now().duration_since(act.hb).as_secs() >= HB_TIMEOUT {
                act.reconnection_timer = Some(ctx.run_later(
                    Duration::from_secs(RECONNECTION_TIME_LIMIT),
                    |_, ctx| {
                        ctx.stop();
                    },
                ));
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
                    self.server.do_send(RegisterSession {
                        addr: ctx.address(),
                        id,
                        server_id: self.server_id,
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
    fn stopped(&mut self, ctx: &mut Self::Context) {
        if let Some(spawn_handle) = self.reconnection_timer {
            ctx.cancel_future(spawn_handle);
        }
        if let Some(id) = self.id.take() {
            self.server.do_send(DeregisterSession {
                server_id: self.server_id,
                id,
                /* Removal reason in this message is only used if the client was still in a room at
                 * the time of termination which can only be possible due to either a network
                 * disconnection or a crash on the client side. Upon normal termination, the client
                 * is expected to send a room leaving message before terminating.*/
                reason: RemoveReason::Disconnected,
                address: ctx.address(),
            });
        }
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

#[derive(Message)]
#[rtype(result = "()")]
pub struct Stop;

impl Handler<Stop> for Session {
    type Result = ();
    fn handle(&mut self, _: Stop, ctx: &mut Self::Context) -> Self::Result {
        if let Some(spawn_handle) = self.reconnection_timer {
            ctx.cancel_future(spawn_handle);
        }
        ctx.stop();
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct JoinRoomResult {
    pub room: Result<(String, Addr<Room>), crate::room::JoinRoomError>,
}

impl Handler<JoinRoomResult> for Session {
    type Result = ();
    fn handle(&mut self, msg: JoinRoomResult, ctx: &mut Self::Context) -> Self::Result {
        let result = match msg.room {
            Ok((code, addr)) => {
                self.room = Some(addr);
                OutgoingMessage::Result {
                    result_of: message::ResultOf::JoinRoom,
                    success: true,
                    info: code,
                }
            }
            Err(err) => OutgoingMessage::Result {
                result_of: message::ResultOf::JoinRoom,
                success: false,
                info: serde_json::to_string(&err)
                    .expect("failed to serialize room join failure reason"),
            },
        };
        let result = serde_json::to_string(&result).expect("Failed to serialise result!");
        ctx.text(result);
    }
}

/* This should only be used when forcefully removing a client from a room due to reasons such
 * disconnection, room expiry, game over, etc. Willingly leaving a room should be handled more
 * gracefully by the client by clearing the self.room field before requesting to be removed from a
 * room.
 * */
#[derive(Message)]
#[rtype(result = "()")]
pub struct ClearRoom {
    pub reason: RemoveReason
}

impl Handler<ClearRoom> for Session {
    type Result = ();
    fn handle(&mut self, msg: ClearRoom, ctx: &mut Self::Context) -> Self::Result {
        let _ = self.room.take();
        let msg = OutgoingMessage::RemoveFromRoom(msg.reason);
        let msg = serde_json::to_string(&msg).unwrap();
        ctx.text(msg);
    }
}
