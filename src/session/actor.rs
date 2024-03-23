use crate::room::actor::JoinRoomError;
use crate::room::{JoinRoom, RoomManager, RoomPair, ROOM_CODE_LENGTH};
use actix::prelude::*;
use actix_web_actors::ws::{self, ProtocolError, WebsocketContext};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::{message::ResultOf, RoomCode};

use super::message::{IncomingMessage, OutgoingMessage};
use super::{Register, Room, SessionManager, UpdateSessionRoomInfo};
use super::{TransientId, Unregister};
use crate::session::message::RemoveReason;

pub type UserId = Arc<str>;

/// How long should we wait before completely disconnecting the client if inactive
const RECONNECTION_TIME_LIMIT: u64 = 15;
/// How frequently should the client check for staleness
const HB_CHECK_INTERVAL: u64 = 5;
/// How frequently should the client send heartbeat messages
const HB_TIME_LIMIT: u64 = 2;

/// Client session responsible for keeping track of client identity,
/// handling client messages, etc
pub struct Session {
    /// Client id that is used to identify the client using external authentication service providers
    id: Option<UserId>,
    /// Server id that is used to identify the stream the client is connected over
    /// (this is a transient id and is not persisted)
    transient_id: Option<TransientId>,
    /// Last recorded heartbeat time
    hb: Instant,
    /// Address of the [Server] actor
    session_manager: Addr<SessionManager>,
    /// [SpawnHandle] of the reconnection timer if the client has disconnected
    /// If the client doesnt reconnect before this timer runs out, the client
    /// will be removed from any rooms they might be in
    reconnection_timer: Option<SpawnHandle>,
    /// [Addr] of the [Room] actor, if the client is in a room
    room: Option<Addr<Room>>,
    room_manager: Addr<RoomManager>,
}

impl Session {
    pub fn new(
        session_manager: Addr<SessionManager>,
        room_manager: Addr<RoomManager>,
    ) -> Self {
        Self {
            room_manager,
            transient_id: None,
            id: None,
            hb: Instant::now(),
            session_manager,
            reconnection_timer: None,
            room: None,
        }
    }
    /// checks for ping every [HB_CHECK_INTERVAL] seconds.
    /// If the last ping was recorded earlier than [HB_TIME_LIMIT] seconds ago, then the
    /// client must have disconnected or have had some kind of network interruption
    fn heartbeat(&mut self, ctx: &mut <Self as Actor>::Context) {
        ctx.run_interval(Duration::from_secs(HB_CHECK_INTERVAL), |act, ctx| {
            if Instant::now().duration_since(act.hb).as_secs() >= HB_TIME_LIMIT {
                act.reconnection_timer = Some(ctx.run_later(
                    Duration::from_secs(RECONNECTION_TIME_LIMIT),
                    |_, ctx| {
                        // This task is cancelled when the client reconnects with another stream.
                        // See [Stop]
                        ctx.stop();
                    },
                ));
            }
        });
    }
    fn join_room(&mut self, code: Option<RoomCode>, ctx: &mut <Self as Actor>::Context) {
        self.room_manager
            .send(JoinRoom {
                session: (
                    self.transient_id.expect("must be registered"),
                    ctx.address(),
                ),
                code,
            })
            .into_actor(self)
            .then(|res, act, ctx| {
                let result = match res {
                    Ok(res) => match res {
                        Ok(RoomPair { code, addr }) => {
                            act.room = Some(addr.clone());
                            act.session_manager.do_send(UpdateSessionRoomInfo(
                                act.transient_id.expect("must be registered"),
                                Some(addr),
                            ));
                            super::message::result(ResultOf::JoinRoom, true, &code)
                        }
                        Err(err) => super::message::result(ResultOf::JoinRoom, false, &err),
                    },
                    Err(err) => {
                        log::error!("{err}");
                        super::message::result(
                            ResultOf::JoinRoom,
                            false,
                            &JoinRoomError::InternalServerError,
                        )
                    }
                };
                ctx.text(result);
                actix::fut::ready(())
            })
            .wait(ctx);
    }
    fn handle_message(&mut self, msg: IncomingMessage, ctx: &mut <Self as Actor>::Context) {
        match msg {
            IncomingMessage::Login(id) => {
                if let Some(_) = &self.id {
                    log::error!("attempting to re-login");
                } else {
                    let id = Arc::from(id);
                    self.id = Some(Arc::clone(&id));
                    self.session_manager
                        .send(Register {
                            session_addr: ctx.address(),
                            user_id: id,
                        })
                        .into_actor(self)
                        .then(|res, act, _| {
                            match res {
                                Ok(transient_id) => act.transient_id = Some(transient_id),
                                Err(err) => log::error!("{err}"),
                            }
                            actix::fut::ready(())
                        })
                        .wait(ctx);
                }
            }
            IncomingMessage::Logout => {
                if let Some(transient_id) = self.transient_id.take() {
                    self.id.take();
                    let reason = RemoveReason::Logout;
                    self.session_manager.do_send(Unregister {
                        transient_id,
                        reason,
                    });
                }
                ctx.stop();
            }
            IncomingMessage::JoinRoom(code) => {
                let res = code.map_or(Ok(None), |code| {
                    string_to_code(code).map_or_else(
                        |_| {
                            ctx.text(super::message::result(
                                ResultOf::JoinRoom,
                                false,
                                &JoinRoomError::InvalidCode,
                            ));
                            Err(())
                        },
                        |chars| Ok(Some(chars)),
                    )
                });
                if let Ok(code) = res {
                    self.join_room(code, ctx)
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
        // Upon normal termination, the sessions id should be removed before disconnection,
        // if not done so, it means something probably went wrong and therefore should be notified
        // to the session_manager and to any related rooms
        if let Some(transient_id) = self.transient_id.take() {
            self.session_manager.do_send(Unregister {
                transient_id,
                /* Removal reason in this message is only used if the client was still in a room at
                 * the time of termination which can only be possible due to either a network
                 * disconnection or a crash on the client side. Upon normal termination, the client
                 * is expected to send a room leaving message before terminating.*/
                reason: RemoveReason::Disconnected,
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

/// Sent by the session_manager in the event where the client reconnects from a different stream.
/// This message is required because the older session controller (the one receiving this message)
/// might have a reconnection timer, which upon evaluating will result in the permanent removal of
/// the client from any rooms they might be in before losing connection.
#[derive(Message)]
#[rtype(result = "()")]
pub struct Stop;

impl Handler<Stop> for Session {
    type Result = ();
    fn handle(&mut self, _: Stop, ctx: &mut Self::Context) -> Self::Result {
        // ID should be removed upon normal termination
        self.id.take();
        ctx.stop();
    }
}

/// This should only be used when forcefully removing a client from a room due to reasons such
/// disconnection, room expiry, game over, etc. Willingly leaving a room should be handled more
/// gracefully by the client by clearing the self.room field before requesting to be removed from a
/// room.
#[derive(Message)]
#[rtype(result = "()")]
pub struct ClearRoom {
    pub reason: RemoveReason,
}

impl Handler<ClearRoom> for Session {
    type Result = ();
    fn handle(&mut self, msg: ClearRoom, ctx: &mut Self::Context) -> Self::Result {
        let _ = self.room.take();
        let msg = OutgoingMessage::RemoveFromRoom(msg.reason);
        let msg = serde_json::to_string(&msg).unwrap();
        ctx.text(msg);
        self.session_manager.do_send(UpdateSessionRoomInfo(
            self.transient_id.expect("must be registered"),
            None,
        ));
    }
}

#[derive(Message, serde::Serialize)]
#[rtype(result = "()")]
pub struct RestoreState {
    pub code: RoomCode,
    pub game: Option<crate::game::SerializedState>,
}

impl Handler<RestoreState> for Session {
    type Result = ();
    fn handle(&mut self, msg: RestoreState, ctx: &mut Self::Context) -> Self::Result {
        match serde_json::to_string(&msg) {
            Ok(res) => ctx.text(res),
            Err(err) => log::error!("game state serialization failed! {err}"),
        }
    }
}

fn code_to_string(code: &[char]) -> Result<String, ()> {
    if code.len() != ROOM_CODE_LENGTH {
        Err(())
    } else {
        let mut string = String::new();
        for char in code {
            string.push(*char);
        }
        Ok(string)
    }
}

fn string_to_code(str: &str) -> Result<[char; ROOM_CODE_LENGTH], ()> {
    if str.len() != ROOM_CODE_LENGTH {
        Err(())
    } else {
        let mut buf = ['0'; ROOM_CODE_LENGTH];
        for (i, char) in str.chars().enumerate() {
            buf[i] = char;
        }
        Ok(buf)
    }
}
