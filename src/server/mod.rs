use crate::room::{self, AddPlayer, ClientReconnection, JoinRoomError, PlayerInRoom, Room};
use crate::session::{Session, UserId};
use actix::{Actor, Addr, AsyncContext, Context, Handler, Message};
use fxhash::FxHashMap;
use rand::Rng;
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
                            server_id: msg.server_id,
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
                    server_id: msg.server_id,
                });
                todo!("send client disconnection message to room!");
            }
            self.sessions_inverted.remove(&data.id);
        }
    }
}

#[derive(Message)]
#[rtype(result = "Result<(), JoinRoomError>")]
pub struct JoinRoom {
    server_id: u128,
    code: Option<RoomCode>,
}

impl Handler<JoinRoom> for Server {
    type Result = Result<(), JoinRoomError>;
    fn handle(&mut self, msg: JoinRoom, ctx: &mut Self::Context) -> Self::Result {
        let result = if let Some(player) = self.sessions.get(&msg.server_id) {
            let info = PlayerInRoom {
                id: player.id.clone(),
                addr: player.addr.clone(),
            };
            let server_id = msg.server_id;
            /* If the message contains a room code, then we look for that room in both private and
             * public room pools. */
            if let Some(code) = msg.code {
                if let Some(RoomInfo {
                    addr,
                    playing,
                    full,
                    ..
                }) = self.private_rooms.get(&code)
                {
                    if *playing {
                        Err(JoinRoomError::GameInProgress)
                    } else if *full {
                        Err(JoinRoomError::RoomFull)
                    } else {
                        addr.do_send(AddPlayer { info, server_id });
                        /* Even though this is an Ok() value, it doesnt necessarily mean that the room
                         * joining operation was successful since we can't guarantee that the chosen room has
                         * space for new players due to the possibility of multiple pending join requests
                         * in its message queue. At this point, we have only succesfully found a
                         * *potentially* joinable room for the user. Therefore, we send the final Success
                         * message from the room's AddPlayer message handler itself. */
                        Ok(())
                    }
                } else if let Some(RoomInfo { addr, .. }) = self.public_matching_pool.get(&code) {
                    addr.do_send(AddPlayer { info, server_id });
                    Ok(())
                } else if let Some(RoomInfo { playing, full, .. }) = self.public_rooms.get(&code) {
                    if *playing {
                        Err(JoinRoomError::GameInProgress)
                    } else if *full {
                        Err(JoinRoomError::RoomFull)
                    } else {
                        panic!("A public room cannot be out of the matching pool unless its full or has a game running in it!");
                    }
                } else {
                    Err(JoinRoomError::RoomNotFound)
                }
            } else {
                /* Otherwise, the user probably wants to join a random room.
                 * This might involve complex matchmaking algorithms which should be injected here
                 * as necessary.
                 * By default we add the player to the first open public room we can find.
                 */
                self.public_matching_pool
                    .iter()
                    .find(|_| /* match criteria */ true)
                    .map_or_else(
                        || {
                            ctx.address().do_send(CreateRoom {
                                server_id: msg.server_id,
                                public: true,
                            });
                        },
                        |(_, room)| {
                            room.addr.do_send(AddPlayer { info, server_id });
                        },
                    );
                Ok(())
            }
        } else {
            Err(JoinRoomError::NotSignedIn)
        };
        result
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct CreateRoom {
    server_id: u128,
    public: bool,
}

impl Handler<CreateRoom> for Server {
    type Result = ();
    fn handle(&mut self, msg: CreateRoom, ctx: &mut Self::Context) -> Self::Result {
        if let Some(data) = self.sessions.get_mut(&msg.server_id) {
            let code = generate_room_id();
            let room = Room::new(
                code.clone(),
                ctx.address(),
                data.id.clone(),
                data.addr.clone(),
                msg.server_id,
                5,
                msg.public,
            )
            .start();
            data.room = Some(room.clone());
            let room = RoomInfo {
                addr: room,
                full: false,
                playing: false,
            };
            data.addr.do_send(crate::session::JoinRoomResult {
                room: Ok((code.clone(), room.addr.clone())),
            });
            if msg.public {
                self.public_rooms.insert(Arc::clone(&code), room);
            }
        }
    }
}

pub enum RoomUnavailablityReason {
    Full,
    GameStarted,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct UpdateRoomMatchAvailability {
    pub code: Arc<str>,
    pub reason: RoomUnavailablityReason,
}

impl Handler<UpdateRoomMatchAvailability> for Server {
    type Result = ();
    fn handle(&mut self, msg: UpdateRoomMatchAvailability, _: &mut Self::Context) -> Self::Result {
        let UpdateRoomMatchAvailability { code, reason } = msg;
        if let Some(mut room) = self.public_matching_pool.remove(&code) {
            match reason {
                RoomUnavailablityReason::Full => room.full = true,
                RoomUnavailablityReason::GameStarted => room.playing = true,
            }
            self.public_rooms.insert(code, room);
        } else if let Some(room) = self.private_rooms.get_mut(&code) {
            match reason {
                RoomUnavailablityReason::Full => room.full = true,
                RoomUnavailablityReason::GameStarted => room.playing = true,
            }
        }
    }
}

/// Rooms notify the server of their stopping so that the server can remove said room from its
/// matching queue.
#[derive(Message)]
#[rtype(result = "()")]
pub struct OnRoomClosed(pub Arc<str>);

impl Handler<OnRoomClosed> for Server {
    type Result = ();
    fn handle(&mut self, msg: OnRoomClosed, _: &mut Self::Context) -> Self::Result {
        if self.public_rooms.remove(&msg.0).is_none() {
            if self.public_matching_pool.remove(&msg.0).is_none() {
                self.private_rooms.remove(&msg.0);
            }
        }
    }
}

/// Sessions notify the server when they join or leave a room.
#[derive(Message)]
#[rtype(result = "()")]
pub struct UpdateSessionRoomInfo(pub u128, pub Option<UserId>);

impl Handler<UpdateSessionRoomInfo> for Server {
    type Result = ();
    fn handle(&mut self, msg: UpdateSessionRoomInfo, _: &mut Self::Context) -> Self::Result {
        if let Some(session_info) = self.sessions.get_mut(&msg.0) {
            session_info.room = msg.1.map(|code| {
                self.public_rooms
                    .get(&code)
                    .unwrap_or(
                        self.public_matching_pool.get(&code).unwrap_or(
                            self.private_rooms
                                .get(&code)
                                .expect("Room must be in one of the pools"),
                        ),
                    )
                    .addr
                    .clone()
            });
        }
    }
}

fn generate_room_id() -> RoomCode {
    const CHARSET: &'static [u8] = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789".as_bytes();
    let mut arr = [0; ROOM_CODE_LENGTH];
    let mut thread_rng = rand::thread_rng();
    for i in 0..ROOM_CODE_LENGTH {
        let r = thread_rng.gen_range(0..CHARSET.len());
        arr[i] = CHARSET[r];
    }
    // We can guarantee that any room code we generate will be valid utf8 strings since we only use
    // ASCII characters from an available character set.
    unsafe { Arc::from(std::str::from_utf8_unchecked(&arr)) }
}
