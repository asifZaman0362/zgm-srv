use crate::room::actor::{PlayerInRoom, Room};
use crate::session::actor::Session;
use crate::session::TransientId;
use actix::{Actor, Addr};
use serde::Serialize;
use std::collections::HashMap;

/// Game state for client side state restoration upon reconnection */
#[derive(Serialize)]
pub struct SerializedState {}

/// State tied to individual players such as their score
struct PlayerState {
    score: usize,
    id: TransientId,
}

/// Common game state, that applies to all game modes
struct GameState {
    players: HashMap<Addr<Session>, PlayerState>,
}

pub struct Game {
    /// Common game state, that applies to all game modes
    state: GameState,
    /// Address([`Addr`]) of the parent room the game is running in
    room: Addr<Room>,
    /// Mode specific game controller. This is where the real action happens
    controller: Box<dyn GameController<Context = <Room as Actor>::Context>>,
}

impl From<&PlayerInRoom> for PlayerState {
    fn from(value: &PlayerInRoom) -> Self {
        Self {
            score: Default::default(),
            id: value.transient_id,
        }
    }
}

impl Game {
    pub fn new(
        room: Addr<Room>,
        player_iter: &Vec<Option<PlayerInRoom>>,
        game_mode: GameMode,
    ) -> Self {
        let mut players = HashMap::new();
        for (addr, state) in player_iter.iter().filter(|x| x.is_some()).map(|player| {
            (
                player.as_ref().unwrap().addr.clone(),
                PlayerState::from(player.as_ref().unwrap()),
            )
        }) {
            players.insert(addr.clone(), state);
        }
        let state = GameState { players };
        let controller = Box::new(match game_mode {
            GameMode::Standard => StandardGame {},
        });
        Self {
            state,
            room,
            controller,
        }
    }
    pub fn stop(&mut self, ctx: &mut <Room as Actor>::Context) {
        self.controller.on_end(ctx);
    }
    pub fn get_state(&self) -> SerializedState {
        SerializedState {}
    }
    pub fn update_session_info(&mut self, old: Addr<Session>, new: (TransientId, Addr<Session>)) {
        if let Some(mut state) = self.state.players.remove(&old) {
            state.id = new.0;
            self.state.players.insert(new.1, state);
        }
    }
}

/// Available game modes
#[derive(Clone, Copy)]
pub enum GameMode {
    Standard,
}

impl Default for GameMode {
    fn default() -> Self {
        GameMode::Standard
    }
}

/// Game Controller interface for game logic
///
/// Game modes can be defined using a game controller trait which exhibits necessary behaviour,
/// the details of which can be dynamic. Note that the following trait only provides a basic outline
/// which might not always be applicable to all use cases. The design of this trait is entirely
/// dependant on the game's design.
pub trait GameController {
    /// The [`actix::Context`] of the parent [`Actor`]
    type Context;
    /// Called when the game is first started
    fn on_start(&self, ctx: &mut Self::Context);
    /// Called when the game ends
    fn on_end(&self, ctx: &mut Self::Context);
}

/// Example implementation of a game mode
struct StandardGame {}

impl GameController for StandardGame {
    type Context = <Room as Actor>::Context;
    fn on_start(&self, _ctx: &mut Self::Context) {}
    fn on_end(&self, _ctx: &mut Self::Context) {}
}
