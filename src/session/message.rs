use serde::{Serialize, Deserialize};
use std::sync::Arc;

#[derive(Deserialize)]
pub enum IncomingMessage<'a> {
    Login(&'a str),
    JoinRoom(&'a str),
    Logout,
    // Add more types here
}

#[derive(Serialize)]
pub enum RemoveReason {
    RoomClosed,
    Logout,
    Disconnected,
    LeaveRequested,
    IdMismatch
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
        info: Arc<str>
    }
    // Add more types here
}
