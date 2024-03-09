use crate::game::Game;
use crate::session::{
    message::{OutgoingMessage, RemoveReason},
    SerializedMessage, Session,
};
use actix::{Actor, ActorContext, Addr, Context, Handler, Message};
use std::collections::HashMap;

pub struct PlayerInRoom {
    pub id: String,
    pub addr: Addr<Session>,
    // extra_info: Info
}

pub struct Room {
    players: HashMap<Addr<Session>, PlayerInRoom>,
    game: Option<Game>,
    max_player_count: usize,
    // further configuration / extra state
}

impl Actor for Room {
    type Context = Context<Self>;
    fn stopped(&mut self, ctx: &mut Self::Context) {
        if let Some(game) = &mut self.game {
            game.stop(ctx)
        }
        for (addr, _) in self.players.iter() {
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
    info: PlayerInRoom,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct RemovePlayer {
    addr: Addr<Session>,
    reason: RemoveReason,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct CloseRoom;

pub enum JoinRoomError {
    RoomFull,
    GameInProgress,
    AlreadyInRoom,
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
            if self.players.get(&msg.info.addr).is_some() {
                Err(JoinRoomError::AlreadyInRoom)
            } else {
                self.players.insert(msg.info.addr.clone(), msg.info);
                Ok(())
            }
        };
        result
    }
}

impl Handler<RemovePlayer> for Room {
    type Result = ();
    fn handle(&mut self, msg: RemovePlayer, ctx: &mut Self::Context) -> Self::Result {
        if let Some(PlayerInRoom { addr, .. }) = self.players.remove(&msg.addr) {
            addr.do_send(SerializedMessage(OutgoingMessage::RemoveFromRoom(
                msg.reason,
            )))
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
