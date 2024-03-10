use crate::room::{self, AddPlayer, JoinRoomError, PlayerInRoom, Room};
use crate::session::Session;
use actix::{Actor, Addr, AsyncContext, Context, Handler, Message};
use fxhash::FxHashMap;
use rand::Rng;

pub mod http;

struct SessionData {
    id: String,
    room: Option<Addr<Room>>,
    addr: Addr<Session>,
}

struct RoomInfo {
    code: String,
    addr: Addr<Room>,
}

// Choice of hashing functions are machine dependant in some cases so it is advised to do your own
// research before picking one (we use multiple here for different purposes).
pub struct Server {
    /* A hashmap of sessions keyed by their server id.
     * See: session/mod.rs for more information on server ids.
     * NOTE: this uses FxHash instead of the standard hash function because this
     * does not need to cryptographic AND I have found that fxhash is the fastest when hashing
     * integers.
     */
    sessions: FxHashMap<u128, SessionData>,
    /* An inverted session map, used for quickly accessing session information using their client
     * ids which is useful for reconnections / API queries. */
    sessions_inverted: ahash::HashMap<String, u128>,
    /* List of all public rooms that *CANNOT* be joined, either due to having been filled completely
     * or due to having games that are currently in progress.
     * NOTE: Rooms must notify the server that they are no longer available for joining when such an
     * event occurs (Room being Filled, Game Starting, etc).
     * Same applies to private rooms.
     */
    public_rooms: ahash::HashMap<String, RoomInfo>,
    /* List of all private rooms that *CANNOT* be joined, due to similar reasons as public room
     * inavailability */
    private_rooms: ahash::HashMap<String, RoomInfo>,
    // List of all public rooms available for matching
    public_matching_pool: ahash::HashMap<String, RoomInfo>,
    // List of all private rooms that can be joined
    private_matching_pool: ahash::HashMap<String, RoomInfo>,
}

use crate::utils::{new_fast_hashmap, new_fx_hashmap};

impl Server {
    pub fn new() -> Self {
        Self {
            sessions: new_fx_hashmap(65536),
            sessions_inverted: new_fast_hashmap(65536),
            public_rooms: new_fast_hashmap(1 << 12),
            private_rooms: new_fast_hashmap(1 << 12),
            private_matching_pool: new_fast_hashmap(1 << 12),
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
    pub id: String,
    pub server_id: u128,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct DeregisterSession {
    pub server_id: u128,
    pub id: String,
    pub address: Addr<Session>,
    pub reason: crate::session::message::RemoveReason,
}

impl Handler<RegisterSession> for Server {
    type Result = ();
    fn handle(&mut self, msg: RegisterSession, _: &mut Self::Context) -> Self::Result {
        if let Some(_) = self.sessions.get(&msg.server_id) {
            log::error!("cannot update client ID while still logged in!");
        } else {
            if let Some(session) = self.sessions_inverted.get_mut(&msg.id) {
                // We remove the older session entry from the session map
                if let Some(mut data) = self.sessions.remove(session) {
                    /* This is a reconnection, let the room that the client was in before disconnecting
                     * (if any) know of the updated session address. */
                    if let Some(room) = &data.room {
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
                    addr: msg.address,
                    reason: msg.reason,
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
    code: Option<String>,
}

impl Handler<JoinRoom> for Server {
    type Result = Result<(), JoinRoomError>;
    fn handle(&mut self, msg: JoinRoom, ctx: &mut Self::Context) -> Self::Result {
        let result = if let Some(player) = self.sessions.get(&msg.server_id) {
            let info = PlayerInRoom {
                id: player.id.clone(),
                addr: player.addr.to_owned(),
            };
            /* If the message contains a room code, then we look for that room in both private and
             * public room pools. */
            if let Some(code) = msg.code {
                if let Some(RoomInfo { addr, .. }) = self.private_rooms.get(&code) {
                    addr.do_send(AddPlayer { info });
                    /* Even though this is an Ok() value, it doesnt necessary mean that the room
                     * joining operation was successful since we can't guarantee that the chosen room has
                     * space for new players due to the possibility of multiple pending join requests
                     * in its message queue. At this point, we have only succesfully found a
                     * *potentially* joinable room for the user. Therefore, we send the final Success
                     * message from the room's AddPlayer message handler itself. */
                    Ok(())
                } else if let Some(RoomInfo { addr, .. }) = self.public_rooms.get(&code) {
                    addr.do_send(AddPlayer { info });
                    Ok(())
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
                            room.addr.do_send(AddPlayer { info });
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
                5,
            )
            .start();
            data.room = Some(room.clone());
            let room = RoomInfo {
                code: code.clone(),
                addr: room,
            };
            data.addr.do_send(crate::session::JoinRoomResult {
                room: Ok((code.clone(), room.addr.clone())),
            });
            if msg.public {
                self.public_rooms.insert(code, room);
            }
        }
    }
}

fn generate_room_id() -> String {
    let mut rng = rand::thread_rng();
    static CHARSET: &'static [u8] = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789".as_bytes();
    (0..6)
        .map(|_| CHARSET[rng.gen_range(0..CHARSET.len())] as char)
        .collect::<String>()
}
