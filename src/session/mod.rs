use crate::{
    room::{
        actor::{ClientReconnection, RemovePlayer, Room},
        RoomCode,
    },
    session::{actor::Session, message::RemoveReason},
    utils::new_fast_hashmap,
};
use actix::prelude::*;
use ahash::HashMap;
use std::sync::Arc;

pub mod actor;
pub mod message;

pub type UserId = Arc<str>;
pub type TransientId = u64;

struct SessionData {
    /// The actor [Addr] of a [Session]
    session_addr: Addr<Session>,
    /// The transient ID is a serializable version of the actors address
    /// It is guaranteed to be unqiue for every session stream
    transient_id: TransientId,
    /// The [Addr] of the [Room] the session is currently in, if in one
    room_addr: Option<Addr<Room>>,
}

/// Atomic session manager
/// Sessions must register themselves on the session manager before beginning regular server
/// interaction
pub struct SessionManager {
    sessions: HashMap<UserId, SessionData>,
    transient_id_map: HashMap<TransientId, UserId>,
    temp_id_counter: TransientId,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: new_fast_hashmap(1 << 12),
            temp_id_counter: 0,
            transient_id_map: new_fast_hashmap(1 << 12),
        }
    }

    pub fn new_id(&mut self) -> TransientId {
        if self.temp_id_counter >= 10_000_000_000 {
            self.temp_id_counter = 0;
        }
        self.temp_id_counter += 1;
        self.temp_id_counter
    }

    pub fn add_session(
        &mut self,
        client_id: UserId,
        session_addr: Addr<Session>,
        transient_id: TransientId,
    ) {
        if let Some(old) = self.sessions.get_mut(&client_id) {
            if let Some(room) = &old.room_addr {
                room.do_send(ClientReconnection {
                    replacee: old.transient_id,
                    replacer: (transient_id, session_addr.clone()),
                });
            }
            old.transient_id = transient_id;
            old.session_addr = session_addr;
        } else {
            self.sessions.insert(
                client_id,
                SessionData {
                    room_addr: None,
                    session_addr,
                    transient_id,
                },
            );
        }
    }

    pub fn remove_session(&mut self, transient_id: TransientId, reason: RemoveReason) {
        if let Some(client_id) = self.transient_id_map.remove(&transient_id) {
            if let Some(SessionData {
                transient_id,
                room_addr,
                ..
            }) = self.sessions.remove(&client_id)
            {
                if let Some(room) = room_addr {
                    room.do_send(RemovePlayer {
                        transient_id,
                        reason,
                    });
                }
            }
        }
    }

    pub fn get_user_by_transient_id(&self, transient_id: TransientId) -> Option<UserId> {
        self.transient_id_map.get(&transient_id).cloned()
    }
}

impl Actor for SessionManager {
    type Context = Context<Self>;
}

#[derive(Message)]
#[rtype(result = "TransientId")]
struct Register {
    session_addr: Addr<Session>,
    user_id: UserId,
}

impl Handler<Register> for SessionManager {
    type Result = TransientId;
    fn handle(&mut self, msg: Register, _: &mut Self::Context) -> Self::Result {
        let transient_id = self.new_id();
        self.add_session(msg.user_id, msg.session_addr, transient_id);
        transient_id
    }
}

#[derive(Message)]
#[rtype(result = "()")]
struct Unregister {
    transient_id: TransientId,
    reason: RemoveReason,
}

impl Handler<Unregister> for SessionManager {
    type Result = ();
    fn handle(&mut self, msg: Unregister, _: &mut Self::Context) -> Self::Result {
        self.remove_session(msg.transient_id, msg.reason);
    }
}

#[derive(Message)]
#[rtype(result = "Option<UserId>")]
struct GetUser(TransientId);

impl Handler<GetUser> for SessionManager {
    type Result = Option<UserId>;
    fn handle(&mut self, msg: GetUser, _: &mut Self::Context) -> Self::Result {
        self.transient_id_map.get(&msg.0).cloned()
    }
}

/// Sessions notify the server when they join or leave a room.
#[derive(Message)]
#[rtype(result = "()")]
pub struct UpdateSessionRoomInfo(pub TransientId, pub Option<Addr<Room>>);

impl Handler<UpdateSessionRoomInfo> for SessionManager {
    type Result = ();
    fn handle(&mut self, msg: UpdateSessionRoomInfo, _: &mut Self::Context) -> Self::Result {
        if let Some(session_info) = self
            .transient_id_map
            .get(&msg.0)
            .and_then(|x| self.sessions.get_mut(x))
        {
            session_info.room_addr = msg.1;
        }
    }
}
