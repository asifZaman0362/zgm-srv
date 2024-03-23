pub mod http;
/*use crate::room::{self, AddPlayer, ClientReconnection, JoinRoomError, PlayerInRoom, Room};
use crate::session::{Session, UserId};
use actix::{Actor, Addr, AsyncContext, Context, Handler, Message};
use fxhash::FxHashMap;
use std::sync::Arc;

pub mod http;

struct SessionData {
    id: UserId,
    room: Option<Addr<Room>>,
    addr: Addr<Session>,
}

struct RoomInfo {
    addr: Addr<Room>,
    playing: bool,
    full: bool,
}

pub const ROOM_CODE_LENGTH: usize = 6;
pub type RoomCode = Arc<str>;
/*
// Choice of hashing functions are machine dependant in some cases so it is advised to do your own
// research before picking one (we use multiple here for different purposes).
/// Main server actor responsible for managing all sessions and rooms.
pub struct Server {
    /// A hashmap of sessions keyed by their server id.
    /// See: session/mod.rs for more information on server ids.
    /// NOTE: this uses FxHash instead of the standard hash function because this
    /// does not need to cryptographic AND I have found that fxhash is the fastest when hashing
    /// integers.
    sessions: FxHashMap<u128, SessionData>,
    /// An inverted session map, used for quickly accessing session information using their client
    /// ids which is useful for reconnections / API queries. */
    sessions_inverted: ahash::HashMap<Arc<str>, u128>,
    /// List of all public rooms that *CANNOT* be joined, either due to having been filled completely
    /// or due to having games that are currently in progress.
    /// NOTE: Rooms must notify the server that they are no longer available for joining when such an
    /// event occurs (Room being Filled, Game Starting, etc).
    /// Same applies to private rooms.
    public_rooms: ahash::HashMap<RoomCode, RoomInfo>,
    /// List of all private rooms
    private_rooms: ahash::HashMap<RoomCode, RoomInfo>,
    /// List of all public rooms available for matching
    public_matching_pool: ahash::HashMap<RoomCode, RoomInfo>,
}

use crate::utils::{new_fast_hashmap, new_fx_hashmap};

impl Server {
    pub fn new() -> Self {
        Self {
            sessions: new_fx_hashmap(65536),
            sessions_inverted: new_fast_hashmap(65536),
            public_rooms: new_fast_hashmap(1 << 12),
            private_rooms: new_fast_hashmap(1 << 12),
            public_matching_pool: new_fast_hashmap(1 << 12),
        }
    }
}

impl Actor for Server {
    type Context = Context<Self>;
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct RegisterSession {
    pub addr: Addr<Session>,
    pub id: Arc<str>,
    pub server_id: u128,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct DeregisterSession {
    pub server_id: u128,
    pub id: Arc<str>,
    pub address: Addr<Session>,
    pub reason: crate::session::message::RemoveReason,
}

impl Handler<RegisterSession> for Server {
    type Result = ();
    fn handle(&mut self, msg: RegisterSession, ctx: &mut Self::Context) -> Self::Result {
        if let Some(session) = self.sessions.get(&msg.server_id) {
            if session.addr == msg.addr {
                log::warn!("probably a mistake on the users end");
            } else {
                log::error!("cannot update client ID while still logged in!");
                session.addr.do_send(crate::session::SerializedMessage(
                    crate::session::message::OutgoingMessage::ForceDisconnect(
                        crate::session::message::RemoveReason::IdMismatch,
                    ),
                ));
                session.addr.do_send(crate::session::Stop);
                ctx.address().do_send(DeregisterSession {
                    reason: crate::session::message::RemoveReason::IdMismatch,
                    id: msg.id,
                    address: msg.addr,
                    server_id: msg.server_id,
                });
            }
        } else {
            if let Some(session) = self.sessions_inverted.get_mut(&msg.id) {
                // We remove the older session entry from the session map
                if let Some(mut data) = self.sessions.remove(session) {
                    /* This is a reconnection, let the room that the client was in before disconnecting
                     * (if any) know of the updated session address. */
                    if let Some(room) = &data.room {
                        room.do_send(ClientReconnection {
                            addr: msg.addr,
                            id: msg.id,
                            transient_id: msg.server_id,
                        });
                        todo!("send client update message");
                    }
                    /* We force the older session actor to terminate
                     * This is necessary since upon ping timeout, the session spawns a new timer
                     * that is set to send a client disconnection message to the server after a
                     * fixed limit which may result in removing the updated connection */
                    data.addr.do_send(crate::session::Stop {});
                    /* Then we push the updated session to the session table because the older
                     * entry was removed. */
                    data.addr = msg.addr.clone();
                    self.sessions.insert(msg.server_id, data);
                    /* And we also update the inverted session map to now point to the updated
                     * session address. */
                    *session = msg.server_id;
                }
            } else {
                // We add the new session to the session maps
                self.sessions.insert(
                    msg.server_id,
                    SessionData {
                        id: msg.id.clone(),
                        room: None,
                        addr: msg.addr.clone(),
                    },
                );
                self.sessions_inverted.insert(msg.id, msg.server_id);
            }
        }
    }
}

/* Permanent disconnection. Clients cannot rejoin a room after this point.
 * Same as a voluntary logout. */
impl Handler<DeregisterSession> for Server {
    type Result = ();
    fn handle(&mut self, msg: DeregisterSession, _: &mut Self::Context) -> Self::Result {
        if let Some(data) = self.sessions.remove(&msg.server_id) {
            if let Some(room) = data.room {
                room.do_send(room::RemovePlayer {
                    reason: msg.reason,
                    transient_id: msg.server_id,
                });
                todo!("send client disconnection message to room!");
            }
            self.sessions_inverted.remove(&data.id);
        }
    }
}
*/
