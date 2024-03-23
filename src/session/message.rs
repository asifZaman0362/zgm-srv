use bytestring::ByteString;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub enum IncomingMessage<'a> {
    Login(&'a str),
    JoinRoom(Option<&'a str>),
    Logout,
    // Add more types here
}

#[derive(Serialize)]
pub enum RemoveReason {
    RoomClosed,
    Logout,
    Disconnected,
    LeaveRequested,
    IdMismatch,
}

#[derive(Serialize)]
pub enum ResultOf {
    JoinRoom,
}

#[derive(Serialize)]
pub enum OutgoingMessage {
    RemoveFromRoom(RemoveReason),
    ForceDisconnect(RemoveReason),
    GameStarted,
    GameEnd,
    /* As much as it hurts my soul, we cant use rust like enum types here because we cant
     * be sure if the client technology is equipped to deserialise such messages. */
    Result {
        result_of: ResultOf,
        success: bool,
        info: String,
    }, // Add more types here
}

pub fn result(result_of: ResultOf, success: bool, info: &impl Serialize) -> OutgoingMessage {
    let info = serde_json::to_string(&info).unwrap();
    OutgoingMessage::Result {
        result_of,
        success,
        info,
    }
}

impl Into<ByteString> for OutgoingMessage {
    fn into(self) -> ByteString {
        ByteString::from(serde_json::to_string(&self).unwrap())
    }
}
