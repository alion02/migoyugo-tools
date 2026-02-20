use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct TimerSettings {
    #[serde(rename = "minutesPerPlayer")]
    pub minutes_per_player: u32,
    #[serde(rename = "incrementSeconds")]
    pub increment_seconds: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LoginRequest<'a> {
    pub email: &'a str,
    pub password: &'a str,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LoginResponse {
    pub message: String,
    pub token: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AcceptChallengeResponse {
    pub success: bool,
    #[serde(rename = "gameId")]
    pub game_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DeclineChallengeResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChallengeReceivedEvent {
    #[serde(rename = "challengeId")]
    pub challenge_id: u64,
    #[serde(rename = "challengerUsername")]
    pub challenger_username: String,
    #[serde(rename = "timerSettings")]
    pub timer_settings: TimerSettings,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GameStartEvent {
    #[serde(rename = "gameId")]
    pub game_id: String,
    #[serde(rename = "playerColor")]
    pub player_color: String,
    #[serde(rename = "opponentName")]
    pub opponent_name: String,
    #[serde(rename = "timerSettings")]
    pub timer_settings: GameTimerSettings,
    pub timers: GameTimers,
    pub timestamp: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GameTimerSettings {
    #[serde(rename = "timerEnabled")]
    pub timer_enabled: bool,
    #[serde(rename = "minutesPerPlayer")]
    pub minutes_per_player: u32,
    #[serde(rename = "incrementSeconds")]
    pub increment_seconds: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GameTimers {
    pub white: u32,
    pub black: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MoveUpdateEvent {
    pub row: u8,
    pub col: u8,
    pub player: String,
    #[serde(rename = "currentPlayer")]
    pub current_player: String,
    #[serde(rename = "gameOver")]
    pub game_over: bool,
    pub winner: Option<String>,
    pub reason: Option<String>,
    pub timers: GameTimers,
    pub timestamp: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GameEndEvent {
    #[serde(rename = "gameId")]
    pub game_id: String,
    pub winner: Option<String>,
    pub reason: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MakeMovePayload {
    #[serde(rename = "gameId")]
    pub game_id: String,
    pub row: u8,
    pub col: u8,
}
