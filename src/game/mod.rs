use crate::room::{PlayerInRoom, Room};
use crate::session::Session;
use actix::{Actor, Addr};
use serde::Serialize;
use std::collections::HashMap;

/* Game state for client side state restoration upon reconnection */
#[derive(Serialize)]
pub struct SerializedState {}

struct PlayerState {
    addr: Addr<Session>,
    id: String,
}

/* @brief Common game state, that apply to all game modes
 */
struct GameState {
    players: HashMap<Addr<Session>, PlayerState>,
}

pub struct Game {
    // Common game state, that apply to all game modes
    state: GameState,
    // Address of the parent room that the game is running in
    room: Addr<Room>,
    // Mode specific game controller, this is where the real action happens
    controller: Box<dyn GameController<Context = <Room as Actor>::Context>>,
}

impl From<&PlayerInRoom> for PlayerState {
    fn from(value: &PlayerInRoom) -> Self {
        Self {
            addr: value.addr.clone(),
            id: value.id.clone(),
        }
    }
}

impl Game {
    pub fn new<'a>(
        room: Addr<Room>,
        player_iter: impl Iterator<Item = &'a PlayerInRoom>,
        game_mode: GameMode,
    ) -> Self {
        let mut players = HashMap::new();
        for player in player_iter.map(|player| PlayerState::from(player)) {
            players.insert(player.addr.clone(), player);
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
}

#[derive(Clone, Copy)]
pub enum GameMode {
    Standard,
}

impl Default for GameMode {
    fn default() -> Self {
        GameMode::Standard
    }
}

/* @brief Game modes can be defined using a game controller trait which exhibits necessary behaviour,
 * the details of which can be dynamic. Note that the following trait only provides a basic outline
 * which might not always be applicable to all use cases. The design of this trait is entirely
 * dependant on the game's design.
 */
pub trait GameController {
    type Context;
    fn on_start(&self, ctx: &mut Self::Context);
    fn on_end(&self, ctx: &mut Self::Context);
    // Add more functions
}

/* @brief Example implementation of a game mode
 * */
struct StandardGame {}

impl GameController for StandardGame {
    type Context = <Room as Actor>::Context;
    fn on_start(&self, _ctx: &mut Self::Context) {}
    fn on_end(&self, _ctx: &mut Self::Context) {}
}
