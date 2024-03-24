use super::RoomCode;
use super::*;
use crate::game::{Game, GameController, GameMode, Input};
use crate::session::TransientId;
use crate::session::{
    actor::{ClearRoom, RestoreState, SerializedMessage, Session},
    message::{OutgoingMessage, RemoveReason},
};
use actix::{Actor, ActorContext, Addr, AsyncContext, Context, Handler, Message};
use ahash::{HashMap, HashMapExt};

pub struct PlayerInRoom {
    pub addr: Addr<Session>,
    pub transient_id: TransientId, // extra_info: Info
}

pub struct GameConfigOptions {
    mode: GameMode,
    // Add extra options
}

impl Default for GameConfigOptions {
    fn default() -> Self {
        Self {
            mode: Default::default(),
        }
    }
}

type Controller =
    dyn GameController<Ctx = Context<Room>, GameInput = Input, SerializedState = String>;

pub struct Room {
    players: Vec<Option<PlayerInRoom>>,
    id_map: HashMap<TransientId, usize>,
    game: Option<Box<Controller>>,
    code: RoomCode,
    room_manager: Addr<RoomManager>, // further configuration / extra state
    game_config: GameConfigOptions,
    room_config: RoomConfig,
    leader: TransientId,
    player_count: usize,
}

impl Room {
    pub fn new(
        code: RoomCode,
        room_manager: Addr<RoomManager>,
        leader: (TransientId, Addr<Session>),
        room_config: RoomConfig,
    ) -> Self {
        let (transient_id, addr) = leader;
        let leader = PlayerInRoom { addr, transient_id };
        let mut id_map = HashMap::with_capacity(room_config.max_player_count as usize);
        id_map.insert(transient_id, 0usize);
        let mut players = Vec::with_capacity(room_config.max_player_count as usize);
        players.push(Some(leader));
        Self {
            players,
            game: None,
            id_map,
            leader: transient_id,
            code,
            room_manager,
            game_config: Default::default(),
            room_config,
            player_count: 1,
        }
    }
    fn start_game(&mut self, ctx: &mut <Self as Actor>::Context) {
        let mut game = Box::new(Game::new(&self.players, self.game_config.mode));
        game.on_begin(ctx);
        self.game = Some(game);
        self.room_manager.do_send(UpdateRoomMatchAvailability {
            code: self.code.clone(),
            availability: Availability::Unavailable(RoomUnavailablityReason::GameStarted),
        });
        self.notify_clients(OutgoingMessage::GameStarted, None);
        todo!("inform players that game has started!");
    }
    pub fn notify_clients(&self, msg: OutgoingMessage, target: Option<usize>) {
        if let Some(idx) = target {
            self.players
                .get(idx)
                .as_ref()
                .expect("target doesnt exist!")
                .as_ref()
                .expect("target cannot be an inactive player!")
                .addr
                .do_send(SerializedMessage(msg));
        } else {
            for player in self.players.iter().filter_map(|x| x.as_ref()) {
                player.addr.do_send(SerializedMessage(msg.clone()));
            }
        }
    }
    pub fn get_id(&self, idx: usize) -> Option<TransientId> {
        self.players
            .get(idx)
            .and_then(|x| x.as_ref().map(|data| data.transient_id))
    }
    pub fn get_players(&self) -> &Vec<Option<PlayerInRoom>> {
        &self.players
    }
}

impl Actor for Room {
    type Context = Context<Self>;
    fn stopped(&mut self, ctx: &mut Self::Context) {
        if let Some(game) = &mut self.game {
            game.on_end(ctx);
        }
        for player in self.players.iter().filter(|x| x.is_some()) {
            let PlayerInRoom { addr, .. } = player.as_ref().unwrap();
            addr.do_send(SerializedMessage(OutgoingMessage::RemoveFromRoom(
                RemoveReason::RoomClosed,
            )));
        }
        self.room_manager.do_send(OnRoomClosed(self.code.clone()));
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct RemovePlayer {
    pub transient_id: TransientId,
    pub reason: RemoveReason,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct CloseRoom;

#[derive(serde::Serialize, Clone)]
pub enum JoinRoomError {
    RoomFull,
    GameInProgress,
    AlreadyInRoom,
    RoomNotFound,
    InvalidCode,
    InternalServerError,
}

#[derive(Message)]
#[rtype(result = "Result<(RoomCode, Addr<Room>), JoinRoomError>")]
pub struct AddPlayer(pub super::SessionPair);

impl Handler<AddPlayer> for Room {
    type Result = Result<(RoomCode, Addr<Room>), JoinRoomError>;
    fn handle(&mut self, msg: AddPlayer, ctx: &mut Self::Context) -> Self::Result {
        let (id, addr) = msg.0;
        /* The default behaviour is to not allow players to join a room while a game is currently
         * in progress in that same room, however it may be deserible to add players to an ongoing
         * game, in which case the following check should be disabled or replaced with some other
         * more appropriate check */
        let result = if self.game.is_some() {
            Err(JoinRoomError::GameInProgress)
        } else if self.player_count >= self.room_config.max_player_count as usize {
            Err(JoinRoomError::RoomFull)
        } else {
            if self.id_map.get(&id).is_some() {
                Err(JoinRoomError::AlreadyInRoom)
            } else {
                if let Some((idx, free)) = self
                    .players
                    .iter_mut()
                    .enumerate()
                    .find(|(_, x)| x.is_none())
                {
                    free.replace(PlayerInRoom {
                        addr,
                        transient_id: id,
                    });
                    self.id_map.insert(id, idx);
                } else {
                    self.id_map.insert(id, self.players.len());
                    self.players.push(Some(PlayerInRoom {
                        addr,
                        transient_id: id,
                    }));
                }
                self.player_count += 1;
                Ok((self.code.clone(), ctx.address()))
            }
        };
        if self.player_count >= self.room_config.max_player_count as usize {
            self.room_manager.do_send(UpdateRoomMatchAvailability {
                code: self.code.clone(),
                availability: Availability::Unavailable(RoomUnavailablityReason::Full),
            });
        }
        result
    }
}

impl Handler<RemovePlayer> for Room {
    type Result = ();
    fn handle(&mut self, msg: RemovePlayer, ctx: &mut Self::Context) -> Self::Result {
        self.player_count -= 1;
        let player = self
            .id_map
            .remove(&msg.transient_id)
            .and_then(|idx| self.players.get_mut(idx).take());
        match msg.reason {
            RemoveReason::LeaveRequested => {
                /* We dont send a ClearRoom message if the client requested a leave since it is
                 * expected from them to already clear their self.room field before requesting a
                 * leave. */
            }
            reason => {
                if let Some(entry) = player {
                    if let Some(PlayerInRoom { addr, .. }) = entry {
                        addr.do_send(ClearRoom { reason });
                    }
                }
            }
        }
        /* It might be desirable to close the room, ending any ongoing games when there are less
         * than however many players are required to keep a game going. Handling this might
         * require further checks that are entirely dependant on the nature of the game itself,
         * therefore such behaviour is not implemented by default.
         * By default, the room is only closed in the event where every participant has left or
         * been removed. */
        if self.player_count == 0 {
            ctx.stop();
        }
    }
}

impl Handler<CloseRoom> for Room {
    type Result = ();
    fn handle(&mut self, _: CloseRoom, ctx: &mut Self::Context) -> Self::Result {
        if let Some(game) = &mut self.game {
            game.on_end(ctx);
        }
        ctx.stop();
    }
}

/* When the client reconnects, it gets a new session address due
 * to having reconnected on a different stream, therefore we must
 * update the stale client address in the room the client was in before
 * disconnecting. This message also sends state information to the
 * reconnecting client address for state restoration. */
#[derive(Message)]
#[rtype(result = "()")]
pub struct ClientReconnection {
    pub replacee: TransientId,
    pub replacer: (TransientId, Addr<Session>),
}

impl Handler<ClientReconnection> for Room {
    type Result = ();
    fn handle(&mut self, msg: ClientReconnection, _: &mut Self::Context) -> Self::Result {
        let ClientReconnection { replacee, replacer } = msg;
        let (new_id, new_addr) = replacer;
        if let Some(idx) = self.id_map.remove(&replacee) {
            if let Some(old) = self.players.get_mut(idx) {
                if let Some(old) = old.take() {
                    new_addr.do_send(RestoreState {
                        code: self.code.clone(),
                        game: self.game.as_mut().map(|g| g.get_state(idx)).unwrap(),
                    });
                }
                self.id_map.insert(new_id, idx);
                *old = Some(PlayerInRoom {
                    addr: new_addr,
                    transient_id: new_id,
                });
            }
        }
    }
}

#[derive(serde::Serialize)]
pub enum StartGameError {
    GameAlreadyRunning,
    NotLeader,
}

#[derive(Message)]
#[rtype(result = "Result<(), StartGameError>")]
pub struct RequestStart(TransientId);

impl Handler<RequestStart> for Room {
    type Result = Result<(), StartGameError>;
    fn handle(&mut self, msg: RequestStart, ctx: &mut Self::Context) -> Self::Result {
        if self.game.is_some() {
            Err(StartGameError::GameAlreadyRunning)
        } else {
            if self.room_config.public {
                self.start_game(ctx);
                Ok(())
            } else {
                if self.leader == msg.0 {
                    self.start_game(ctx);
                    Ok(())
                } else {
                    Err(StartGameError::NotLeader)
                }
            }
        }
    }
}
