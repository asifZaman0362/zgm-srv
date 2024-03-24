use crate::room::actor::{PlayerInRoom, Room};
use crate::session::TransientId;
use actix::Context;
use serde::Serialize;

/// Game state for client side state restoration upon reconnection */
#[derive(Serialize)]
pub struct SerializedState {
    word: String,
    time_remaining: Option<u64>,
    turn: TransientId,
    score: usize,
}

/// State tied to individual players such as their score
struct PlayerState {
    score: usize,
    id: TransientId,
    alive: bool,
}

/// Common game state, that applies to all game modes
pub struct GameState {
    player_data: Vec<Option<PlayerState>>,
    word: String,
    turn: usize,
    timer: Option<(actix::SpawnHandle, std::time::Instant)>,
}

pub struct Game {
    /// Common game state, that applies to all game modes
    state: GameState,
}

impl From<&PlayerInRoom> for PlayerState {
    fn from(value: &PlayerInRoom) -> Self {
        Self {
            score: Default::default(),
            id: value.transient_id,
            alive: true,
        }
    }
}

impl Game {
    pub fn new(players: &Vec<Option<PlayerInRoom>>, game_mode: GameMode) -> Self {
        let player_data = players
            .iter()
            .map(|x| x.as_ref().map(|x| PlayerState::from(x)))
            .collect::<Vec<_>>();
        let state = GameState {
            player_data,
            word: "".to_string(),
            turn: 0,
            timer: None,
        };
        Self { state }
    }
    pub fn get_state(&self, player: usize) -> SerializedState {
        let state = &self.state;
        let word = state.word.clone();
        let time_remaining = state.timer.map(|(_, trigger_time)| {
            trigger_time
                .duration_since(std::time::Instant::now())
                .as_secs()
        });
        let turn = state.turn;
        let score = state
            .player_data
            .get(player)
            .expect("data doesnt exist for player!")
            .as_ref()
            .expect("player data cannot be empty!")
            .score;
        SerializedState {
            word,
            time_remaining,
            turn: turn.try_into().unwrap(),
            score,
        }
    }
}

#[derive(Copy, Clone)]
pub enum GameMode {
    Standard,
}

impl Default for GameMode {
    fn default() -> Self {
        GameMode::Standard
    }
}

/// Example implementation of a game mode
struct StandardGame {}

impl StandardGame {
    fn next_turn(&self, state: &mut GameState, room: &Room) {
        state.turn = if let Some(next) = state.player_data.as_slice()[state.turn..]
            .iter()
            .position(|x| x.as_ref().map_or(false, |state| state.alive))
        {
            next
        } else {
            state
                .player_data
                .iter()
                .position(|x| x.as_ref().map_or(false, |state| state.alive))
                .expect("everyone cannot be dead!")
        };
        room.notify_clients(
            crate::session::message::OutgoingMessage::TurnUpdate(room.get_id(state.turn).unwrap()),
            None,
        );
    }
}

pub trait GameController {
    type Ctx;
    type GameInput;
    type SerializedState;
    fn on_begin(&mut self, ctx: &mut Self::Ctx);
    fn on_end(&mut self, ctx: &mut Self::Ctx);
    fn on_pause(&mut self, ctx: &mut Self::Ctx);
    fn on_resume(&mut self, ctx: &mut Self::Ctx);
    fn on_input(&mut self, ctx: &mut Self::Ctx, input: &Self::GameInput);
    fn get_state(&self, player: usize) -> Self::SerializedState;
}

pub enum Input {
    Word(String)
}

impl GameController for Game {
    type Ctx = Context<Room>;
    type GameInput = Input;
    type SerializedState = String;
    fn on_begin(&mut self, ctx: &mut Self::Ctx) {
    }
    fn on_input(&mut self, ctx: &mut Self::Ctx, input: &Self::GameInput) {
    }
    fn on_pause(&mut self, ctx: &mut Self::Ctx) {
    }
    fn on_resume(&mut self, ctx: &mut Self::Ctx) {
    }
    fn on_end(&mut self, ctx: &mut Self::Ctx) {
    }
    fn get_state<'a>(&'a self, player: usize) -> Self::SerializedState {
        serde_json::to_string(&self.get_state(player)).unwrap()
    }
}

