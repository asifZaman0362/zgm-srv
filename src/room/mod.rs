use crate::game::{Game, GameMode};
use crate::server::{RoomUnavailablityReason, Server, UpdateRoomMatchAvailability};
use crate::session::{
    message::{OutgoingMessage, RemoveReason},
    RestoreState, SerializedMessage, Session,
};
use actix::{Actor, ActorContext, Addr, AsyncContext, Context, Handler, Message};

pub struct PlayerInRoom {
    pub id: String,
    pub addr: Addr<Session>,
    // extra_info: Info
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

pub struct Room {
    players: fxhash::FxHashMap<u128, PlayerInRoom>,
    game: Option<Game>,
    leader: String,
    code: String,
    max_player_count: usize,
    server: Addr<Server>, // further configuration / extra state
    public: bool,
    config: GameConfigOptions,
}

impl Room {
    pub fn new(
        code: String,
        server: Addr<Server>,
        leader_id: String,
        leader_addr: Addr<Session>,
        leader_server_id: u128,
        max_player_count: usize,
        public: bool,
    ) -> Self {
        let leader = PlayerInRoom {
            id: leader_id.clone(),
            addr: leader_addr,
        };
        let mut players = crate::utils::new_fx_hashmap(max_player_count);
        players.insert(leader_server_id, leader);
        Self {
            players,
            game: None,
            leader: leader_id,
            code,
            max_player_count,
            server,
            public,
            config: Default::default(),
        }
    }
    fn start_game(&mut self, ctx: &mut <Self as Actor>::Context) {
        self.game = Some(Game::new(
            ctx.address(),
            self.players.values(),
            self.config.mode,
        ));
        self.server.do_send(UpdateRoomMatchAvailability {
            code: self.code.clone(),
            reason: RoomUnavailablityReason::GameStarted,
        });
        todo!("inform players that game has started!");
    }
}

impl Actor for Room {
    type Context = Context<Self>;
    fn stopped(&mut self, ctx: &mut Self::Context) {
        if let Some(game) = &mut self.game {
            game.stop(ctx)
        }
        for (_, PlayerInRoom { addr, .. }) in self.players.iter() {
            addr.do_send(SerializedMessage(OutgoingMessage::RemoveFromRoom(
                RemoveReason::RoomClosed,
            )));
        }
    }
}

pub type RoomInfo = ();

#[derive(Message)]
#[rtype(result = "Result<RoomInfo, JoinRoomError>")]
pub struct AddPlayer {
    pub server_id: u128,
    pub info: PlayerInRoom,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct RemovePlayer {
    pub server_id: u128,
    pub reason: RemoveReason,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct CloseRoom;

#[derive(serde::Serialize)]
pub enum JoinRoomError {
    RoomFull,
    GameInProgress,
    AlreadyInRoom,
    RoomNotFound,
    NotSignedIn,
}

impl Handler<AddPlayer> for Room {
    type Result = Result<RoomInfo, JoinRoomError>;
    fn handle(&mut self, msg: AddPlayer, _: &mut Self::Context) -> Self::Result {
        /* The default behaviour is to not allow players to join a room while a game is currently
         * in progress in that same room, however it may be deserible to add players to an ongoing
         * game, in which case the following check should be disabled or replaced with some other
         * more appropriate check */
        let result = if self.game.is_some() {
            Err(JoinRoomError::GameInProgress)
        } else if self.players.len() > self.max_player_count {
            Err(JoinRoomError::RoomFull)
        } else {
            if self.players.get(&msg.server_id).is_some() {
                Err(JoinRoomError::AlreadyInRoom)
            } else {
                self.players.insert(msg.server_id, msg.info);
                Ok(())
            }
        };
        if self.players.len() > self.max_player_count {
            self.server.do_send(UpdateRoomMatchAvailability {
                code: self.code.clone(),
                reason: RoomUnavailablityReason::Full,
            });
        }
        result
    }
}

impl Handler<RemovePlayer> for Room {
    type Result = ();
    fn handle(&mut self, msg: RemovePlayer, ctx: &mut Self::Context) -> Self::Result {
        match msg.reason {
            RemoveReason::LeaveRequested => {
                /* We dont send a ClearRoom message if the client requested a leave since it is
                 * expected from them to already clear their self.room field before requesting a
                 * leave. */
            }
            reason => {
                if let Some(PlayerInRoom { addr, .. }) = self.players.remove(&msg.server_id) {
                    addr.do_send(crate::session::ClearRoom { reason });
                }
            }
        }
        /* It might be desirable to close the room, ending any ongoing games when there are less
         * than however many players are required to keep a game going. Handling this might
         * require further checks that are entirely dependant on the nature of the game itself,
         * therefore such behaviour is not implemented by default.
         * By default, the room is only closed in the event where every participant has left or
         * been removed. */
        if self.players.is_empty() {
            ctx.stop();
        }
    }
}

impl Handler<CloseRoom> for Room {
    type Result = ();
    fn handle(&mut self, _: CloseRoom, ctx: &mut Self::Context) -> Self::Result {
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
    pub addr: Addr<Session>,
    pub id: String,
    pub server_id: u128,
}

impl Handler<ClientReconnection> for Room {
    type Result = ();
    fn handle(&mut self, msg: ClientReconnection, _: &mut Self::Context) -> Self::Result {
        match self
            .players
            .iter()
            .find(|(_, PlayerInRoom { id, .. })| id == &msg.id)
            .map(|(id, ..)| *id)
        {
            Some(id) => {
                let mut player = self.players.remove(&id).unwrap();
                msg.addr.do_send(RestoreState {
                    code: self.code.clone(),
                    game: self.game.as_ref().map(|g| g.get_state()),
                });
                player.addr = msg.addr;
                self.players.insert(msg.server_id, player);
            }
            None => {}
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
pub struct RequestStart {
    pub id: String,
}

impl Handler<RequestStart> for Room {
    type Result = Result<(), StartGameError>;
    fn handle(&mut self, msg: RequestStart, ctx: &mut Self::Context) -> Self::Result {
        if self.game.is_some() {
            Err(StartGameError::GameAlreadyRunning)
        } else {
            if self.public {
                Ok(())
            } else {
                if self.leader == msg.id {
                    self.start_game(ctx);
                    Ok(())
                } else {
                    Err(StartGameError::NotLeader)
                }
            }
        }
    }
}
