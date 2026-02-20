use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimerSettings {
    pub minutes_per_player: u32,
    pub increment_seconds: u32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginRequest<'a> {
    pub email: &'a str,
    pub password: &'a str,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginResponse {
    pub message: String,
    pub token: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptChallengeResponse {
    pub success: bool,
    pub game_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeclineChallengeResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChallengeReceivedEvent {
    pub challenge_id: u64,
    pub challenger_username: String,
    pub timer_settings: TimerSettings,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameStartEvent {
    pub game_id: String,
    pub player_color: String,
    pub opponent_name: String,
    pub timer_settings: GameTimerSettings,
    pub timers: GameTimers,
    pub timestamp: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameTimerSettings {
    pub timer_enabled: bool,
    pub minutes_per_player: u32,
    pub increment_seconds: u32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameTimers {
    pub white: u32,
    pub black: u32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveUpdateEvent {
    pub row: u8,
    pub col: u8,
    pub player: String,
    pub current_player: String,
    pub game_over: bool,
    pub winner: Option<String>,
    pub reason: Option<String>,
    pub timers: GameTimers,
    pub timestamp: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameEndEvent {
    pub game_id: String,
    pub winner: Option<String>,
    pub reason: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MakeMovePayload {
    pub game_id: String,
    pub row: u8,
    pub col: u8,
}
