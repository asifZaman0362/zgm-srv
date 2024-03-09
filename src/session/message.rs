use serde::{Serialize, Deserialize};

#[derive(Deserialize)]
pub enum IncomingMessage {
    Login(String),
    // Add more types here
}

#[derive(Serialize)]
pub enum RemoveReason {
    RoomClosed,
    Logout,
    Disconnected
}

#[derive(Serialize)]
pub enum OutgoingMessage {
    RemoveFromRoom(RemoveReason),
    GameStarted,
    GameEnd,
    // Add more types here
}
