use std::collections::HashMap;
use actix::{Actor, Context, Addr, Message, Handler};
use crate::session::Session;

pub mod http;

struct SessionData {
    id: String
}

pub struct Server {
    sessions: HashMap<Addr<Session>, SessionData>
}

impl Actor for Server {
    type Context = Context<Self>;
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct RegisterSession {
    pub addr: Addr<Session>,
    pub id: String
}

impl Handler<RegisterSession> for Server {
    type Result = ();
    fn handle(&mut self, msg: RegisterSession, ctx: &mut Self::Context) -> Self::Result {
    }
}
