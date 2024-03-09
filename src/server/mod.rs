use crate::room::{self, Room};
use crate::session::Session;
use actix::{Actor, Addr, AsyncContext, Context, Handler, Message};
use rand::Rng;
use std::collections::HashMap;

pub mod http;

struct SessionData {
    id: String,
    room: Option<Addr<Room>>,
    addr: Addr<Session>,
}

pub struct Server {
    sessions: HashMap<u128, SessionData>,
    sessions_inverted: HashMap<String, u128>,
    public_rooms: HashMap<String, Addr<Room>>,
    private_rooms: HashMap<String, Addr<Room>>,
}

impl Server {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
            sessions_inverted: HashMap::new(),
            public_rooms: HashMap::new(),
            private_rooms: HashMap::new(),
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
                    // This is a reconnection, let the room that the client was in before disconnecting
                    // (if any) know of the updated session address
                    if let Some(room) = &data.room {
                        todo!("send client update message");
                    }
                    // We force the older session actor to terminate
                    // This is necessary since upon ping timeout, the session spawns a new timer
                    // that is set to send a client disconnection message to the server after a
                    // fixed limit which may result in removing the updated connection
                    data.addr.do_send(crate::session::Stop {});
                    // Then we push the updated session to the session table because the older
                    // entry was removed
                    data.addr = msg.addr.clone();
                    self.sessions.insert(msg.server_id, data);
                    // And we also update the inverted session map to now point to the updated
                    // session address
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

// Permanent disconnection. Clients cannot rejoin a room after this point.
// Same as a voluntary logout.
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
#[rtype(result = "()")]
pub struct JoinRoom {
    server_id: u128,
    code: Option<String>,
}

impl Handler<JoinRoom> for Server {
    type Result = ();
    fn handle(&mut self, msg: JoinRoom, ctx: &mut Self::Context) -> Self::Result {}
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
            todo!("send message to client updating room");
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
