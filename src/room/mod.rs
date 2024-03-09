use crate::session::{message::SocketMessage, Session};
use actix::{Actor, Addr, Context, Handler, Message};
use std::collections::HashMap;

pub trait Game: 'static + Unpin {}
pub trait PlayerInfo: 'static + Unpin {}

struct PlayerInRoom<P: PlayerInfo, M: SocketMessage> {
    id: String,
    addr: Addr<Session<M>>,
    extra_info: P,
}

pub struct Room<P: PlayerInfo, G: Game, M: SocketMessage> {
    players: HashMap<Addr<Session<M>>, PlayerInRoom<P, M>>,
    game: Option<G>,
}

impl<P: PlayerInfo, G: Game, M: SocketMessage> Actor for Room<P, G, M> {
    type Context = Context<Self>;
    fn stopped(&mut self, ctx: &mut Self::Context) {}
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct AddPlayer<P: PlayerInfo, M: SocketMessage> {
    info: PlayerInRoom<P, M>,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct RemovePlayer<M: SocketMessage> {
    addr: Addr<Session<M>>,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct CloseRoom;

impl<P: PlayerInfo, G: Game, M: SocketMessage> Handler<AddPlayer<P, M>> for Room<P, G, M> {
    type Result = ();
    fn handle(&mut self, msg: AddPlayer<P, M>, ctx: &mut Self::Context) -> Self::Result {}
}

impl<P: PlayerInfo, G: Game, M: SocketMessage> Handler<RemovePlayer<M>> for Room<P, G, M> {
    type Result = ();
    fn handle(&mut self, msg: RemovePlayer<M>, ctx: &mut Self::Context) -> Self::Result {}
}

impl<P: PlayerInfo, G: Game, M: SocketMessage> Handler<CloseRoom> for Room<P, G, M> {
    type Result = ();
    fn handle(&mut self, msg: CloseRoom, ctx: &mut Self::Context) -> Self::Result {
    }
}
