use bytestring::ByteString;
use serde::{Deserialize, Serialize};
use crate::{session::TransientId, room::actor::JoinRoomError};

#[derive(Deserialize)]
#[serde(tag = "kind", content = "data")]
pub enum IncomingMessage<'a> {
    Login(&'a str),
    JoinRoom(Option<&'a str>),
    Logout,
    // Add more types here
}

#[derive(Serialize, Clone)]
pub enum RemoveReason {
    RoomClosed,
    Logout,
    Disconnected,
    LeaveRequested,
    IdMismatch,
}

#[derive(Serialize, Clone)]
pub enum ResultOf {
    JoinRoom,
}

#[derive(Serialize, Clone)]
#[serde(tag = "status", content = "data")]
pub enum Result<T, E> {
    Success(T),
    Error(E)
}

#[derive(Serialize, Clone)]
#[serde(tag = "kind", content = "data")]
pub enum OutgoingMessage {
    RemoveFromRoom(RemoveReason),
    ForceDisconnect(RemoveReason),
    GameStarted,
    GameEnd,
    JoinRoomResult(Result<String, JoinRoomError>),
    TurnUpdate(TransientId)
}

impl Into<ByteString> for OutgoingMessage {
    fn into(self) -> ByteString {
        ByteString::from(serde_json::to_string(&self).unwrap())
    }
}
