use serde::{Serialize, Deserialize};

#[derive(Deserialize)]
pub enum IncomingMessage {
    Login(String),
    JoinRoom(String),
    // Add more types here
}

#[derive(Serialize)]
pub enum RemoveReason {
    RoomClosed,
    Logout,
    Disconnected,
    LeaveRequested
}

#[derive(Serialize)]
pub enum ResultOf {
    JoinRoom,
}

#[derive(Serialize)]
pub enum OutgoingMessage {
    RemoveFromRoom(RemoveReason),
    GameStarted,
    GameEnd,
    /* As much as it hurts my soul, we cant use rust like enum types here because we cant 
     * be sure if the client technology is equipped to deserialise such messages. */
    Result {
        result_of: ResultOf,
        success: bool,
        info: String
    }
    // Add more types here
}
