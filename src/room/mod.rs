use actix::prelude::*;
use ahash::HashMap;

use actor::Room;
use fastrand::Rng;

use crate::session::{actor::Session, TransientId};

use self::actor::{AddPlayer, JoinRoomError};
pub mod actor;

pub struct RoomConfig {
    public: bool,
    max_player_count: u8,
}

const DEFAULT_PLAYER_LIMIT: u8 = 6;

impl Default for RoomConfig {
    fn default() -> Self {
        Self {
            public: true,
            max_player_count: DEFAULT_PLAYER_LIMIT,
        }
    }
}

struct RoomInfo {
    addr: Addr<Room>,
    playing: bool,
    full: bool,
}

impl RoomInfo {
    fn new(addr: Addr<Room>) -> Self {
        Self {
            addr,
            playing: false,
            full: false,
        }
    }
    fn reset(&mut self) {
        self.full = false;
        self.playing = false;
    }
}

pub const ROOM_CODE_LENGTH: usize = 4;
pub type RoomCode = [u8; ROOM_CODE_LENGTH];

pub struct RoomManager {
    free: HashMap<RoomCode, RoomInfo>,
    reserved: HashMap<RoomCode, RoomInfo>,
    open: HashMap<RoomCode, RoomInfo>,
}

impl RoomManager {
    pub fn new() -> Self {
        const capacity: usize = 1 << 12;
        let free: HashMap<RoomCode, RoomInfo> = crate::utils::new_fast_hashmap(capacity);
        let reserved: HashMap<RoomCode, RoomInfo> = crate::utils::new_fast_hashmap(capacity);
        let open: HashMap<RoomCode, RoomInfo> = crate::utils::new_fast_hashmap(capacity);
        Self {
            free,
            reserved,
            open,
        }
    }
    fn get_free(&mut self) -> Option<(RoomCode, RoomInfo)> {
        if let Some(code) = self.free.keys().find(|_| true).cloned() {
            Some((code, self.free.remove(&code).unwrap()))
        } else {
            None
        }
    }
    fn release(&mut self, key: RoomCode) {
        self.reserved.remove(&key).map(|x| self.free.insert(key, x));
    }
    fn create(
        &mut self,
        leader: (TransientId, Addr<Session>),
        room_config: RoomConfig,
        room_manager: Addr<Self>,
    ) -> RoomPair {
        let code = generate_room_id();
        let addr = Room::new(code, room_manager, leader, room_config).start();
        let room = RoomInfo::new(addr.clone());
        self.reserved.insert(code, room);
        RoomPair { code, addr }
    }
}

impl Actor for RoomManager {
    type Context = Context<Self>;
}

#[derive(MessageResponse)]
pub struct RoomPair {
    pub code: RoomCode,
    pub addr: Addr<Room>,
}

type SessionPair = (TransientId, Addr<Session>);

#[derive(Message)]
#[rtype(result = "RoomPair")]
struct CreateRoom {
    leader: (TransientId, Addr<Session>),
    room_config: RoomConfig,
}

impl Handler<CreateRoom> for RoomManager {
    type Result = RoomPair;
    fn handle(&mut self, msg: CreateRoom, ctx: &mut Self::Context) -> Self::Result {
        let CreateRoom {
            leader,
            room_config,
        } = msg;
        self.create(leader, room_config, ctx.address())
    }
}

#[derive(Message)]
#[rtype(result = "Result<RoomPair, JoinRoomError>")]
pub struct JoinRoom {
    pub session: SessionPair,
    pub code: Option<RoomCode>,
}

impl Handler<JoinRoom> for RoomManager {
    type Result = ResponseActFuture<Self, Result<RoomPair, JoinRoomError>>;
    fn handle(&mut self, msg: JoinRoom, ctx: &mut Self::Context) -> Self::Result {
        /* If the message contains a room code, then we look for that room in both private and
         * public room pools. */
        if let Some(code) = msg.code {
            if let Some(RoomInfo {
                addr,
                playing,
                full,
                ..
            }) = self.reserved.get(&code)
            {
                if *playing {
                    Box::pin(actix::fut::ready(Err(JoinRoomError::GameInProgress)).into_actor(self))
                } else if *full {
                    Box::pin(actix::fut::ready(Err(JoinRoomError::RoomFull)).into_actor(self))
                } else {
                    Box::pin(addr.send(AddPlayer(msg.session)).into_actor(self).then(
                        |res, _, _| {
                            actix::fut::ready(
                                res.map_or(Err(JoinRoomError::InternalServerError), |res| {
                                    res.map(|(code, addr)| RoomPair { addr, code })
                                }),
                            )
                        },
                    ))
                }
            } else if let Some(RoomInfo { addr, .. }) = self.open.get(&code) {
                Box::pin(
                    addr.send(AddPlayer(msg.session))
                        .into_actor(self)
                        .then(|res, _, _| {
                            actix::fut::ready(
                                res.map_or(Err(JoinRoomError::InternalServerError), |res| {
                                    res.map(|(code, addr)| RoomPair { addr, code })
                                }),
                            )
                        }),
                )
            } else if let Some(RoomInfo { playing, full, .. }) = self.open.get(&code) {
                if *playing {
                    Box::pin(actix::fut::ready(Err(JoinRoomError::GameInProgress)))
                } else if *full {
                    Box::pin(actix::fut::ready(Err(JoinRoomError::RoomFull)))
                } else {
                    panic!("A public room cannot be out of the matching pool unless its full or has a game running in it!");
                }
            } else {
                Box::pin(actix::fut::ready(Err(JoinRoomError::RoomNotFound)))
            }
        } else {
            /* Otherwise, the user probably wants to join a random room.
             * This might involve complex matchmaking algorithms which should be injected here
             * as necessary.
             * By default we add the player to the first open public room we can find.
             */
            if let Some(found) = self.open.iter().find(|_| /* match criteria */ true) {
                Box::pin(
                    found
                        .1
                        .addr
                        .send(AddPlayer(msg.session))
                        .into_actor(self)
                        .then(|res, _, _| {
                            actix::fut::ready(
                                res.map_or(Err(JoinRoomError::InternalServerError), |res| {
                                    res.map(|(code, addr)| RoomPair { addr, code })
                                }),
                            )
                        }),
                )
            } else {
                let info = Ok(self.create(msg.session, Default::default(), ctx.address()));
                Box::pin(actix::fut::ready(info))
            }
        }
    }
}

pub enum RoomUnavailablityReason {
    Full,
    GameStarted,
}

pub enum Availability {
    Available,
    Unavailable(RoomUnavailablityReason),
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct UpdateRoomMatchAvailability {
    pub code: RoomCode,
    pub availability: Availability,
}

impl Handler<UpdateRoomMatchAvailability> for RoomManager {
    type Result = ();
    fn handle(&mut self, msg: UpdateRoomMatchAvailability, _: &mut Self::Context) -> Self::Result {
        let code = msg.code;
        match msg.availability {
            Availability::Available => {
                if let Some(room) = self.reserved.remove(&code) {
                    if !room.full && !room.playing {
                        self.open.insert(code, room);
                    } else {
                        self.reserved.insert(code, room);
                    }
                }
            }
            Availability::Unavailable(reason) => {
                if let Some(mut room) = self.open.remove(&code) {
                    match reason {
                        RoomUnavailablityReason::Full => room.full = true,
                        RoomUnavailablityReason::GameStarted => room.playing = true,
                    }
                    self.reserved.insert(code, room);
                } else if let Some(room) = self.reserved.get_mut(&code) {
                    match reason {
                        RoomUnavailablityReason::Full => room.full = true,
                        RoomUnavailablityReason::GameStarted => room.playing = true,
                    }
                }
            }
        }
    }
}

/// Rooms notify the server of their stopping so that the server can remove said room from its
/// matching queue. Rooms are expected to reset their settings before sending this message.
#[derive(Message)]
#[rtype(result = "()")]
pub struct OnRoomClosed(pub RoomCode);

impl Handler<OnRoomClosed> for RoomManager {
    type Result = ();
    fn handle(&mut self, msg: OnRoomClosed, _: &mut Self::Context) -> Self::Result {
        if let Some(mut room) = self.open.remove(&msg.0).or(self.reserved.remove(&msg.0)) {
            room.reset();
            // Push room onto list of available rooms for pooling
            self.free.insert(msg.0, room);
        }
    }
}

fn generate_room_id() -> RoomCode {
    const CHARSET: &'static [u8] = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789".as_bytes();
    let mut arr = [0; ROOM_CODE_LENGTH];
    let mut rng = Rng::new();
    for i in 0..ROOM_CODE_LENGTH {
        let r = rng.usize(0..CHARSET.len());
        arr[i] = CHARSET[r];
    }
    arr
}
