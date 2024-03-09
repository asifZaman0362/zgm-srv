use serde::{Serialize, Deserialize};

pub trait SocketMessage: 'static + for<'a> Deserialize<'a> + Unpin {}

#[derive(Deserialize)]
pub enum IncomingMessage<T> {
    Login(String),
    Message(T)
}

#[derive(Serialize)]
pub enum RemoveReason {
    RoomClosed,
    Kicked(String)
}

#[derive(Serialize)]
pub enum OutgoingMessage<T: Serialize> {
    RemoveFromRoom(RemoveReason),
    GameStarted,
    GameEnd,
    Message(T)
}
