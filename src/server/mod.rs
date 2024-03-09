use std::collections::HashMap;
use actix::{Actor, Context, Addr};
use crate::session::{Session, message::SocketMessage};

pub mod http;

struct SessionData {
    id: String
}

pub struct Server<M: SocketMessage> {
    sessions: HashMap<Addr<Session<M>>, SessionData>
}

impl<M: SocketMessage> Actor for Server<M> {
    type Context = Context<Self>;
}
