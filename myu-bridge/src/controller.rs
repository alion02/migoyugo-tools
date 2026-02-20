use std::future;
use std::sync::Arc;

use anyhow::Result;
use tokio::sync::mpsc;
use tokio::time::Instant;

use crate::config::AcceptRule;
use crate::engine::{EngineEvent, EngineWrapper, GoCommand};
use crate::http::MigoyugoHttpClient;
use crate::models::{ChallengeReceivedEvent, GameStartEvent, MoveUpdateEvent};
use crate::socket::{BridgeEvent, MigoyugoSocketClient};

pub struct GameState {
    pub game_id: String,
    pub engine: EngineWrapper,
    pub my_color: String,
    pub engine_tx: mpsc::Sender<EngineEvent>,
    pub engine_rx: mpsc::Receiver<EngineEvent>,
    pub pending_challenge_id: Option<u64>,
    pub config_rule: AcceptRule,
    pub last_move_time: Option<Instant>,
}

pub struct Controller {
    pub http_client: Arc<MigoyugoHttpClient>,
    pub socket_client: Arc<MigoyugoSocketClient>,
    pub config_rules: Vec<AcceptRule>,
    pub socket_rx: mpsc::Receiver<BridgeEvent>,

    // Simplification for this bridge: handle one active game at a time.
    pub active_game: Option<GameState>,
}

impl Controller {
    pub fn new(
        http_client: Arc<MigoyugoHttpClient>,
        socket_client: Arc<MigoyugoSocketClient>,
        config_rules: Vec<AcceptRule>,
        socket_rx: mpsc::Receiver<BridgeEvent>,
    ) -> Self {
        Self { http_client, socket_client, config_rules, socket_rx, active_game: None }
    }

    pub async fn run(&mut self) -> Result<()> {
        loop {
            tokio::select! {
                Some(event) = self.socket_rx.recv() => {
                    if let Err(e) = self.handle_socket_event(event).await {
                        tracing::error!("Error handling socket event: {:?}", e);
                    }
                }

                Some(engine_event) = async {
                    if let Some(game) = self.active_game.as_mut() {
                        game.engine_rx.recv().await
                    } else {
                        future::pending().await
                    }
                } => {
                    if let Err(e) = self.handle_engine_event(engine_event).await {
                        tracing::error!("Error handling engine event: {:?}", e);
                    }
                }
            }
        }
    }

    async fn handle_socket_event(&mut self, event: BridgeEvent) -> Result<()> {
        match event {
            BridgeEvent::Connected => {
                tracing::info!("Socket connected");
            }
            BridgeEvent::Disconnected => {
                tracing::warn!("Socket disconnected");
                // The socket.io client handles reconnects, but we might want to clean up state
            }
            BridgeEvent::ChallengeReceived(evt) => {
                self.handle_challenge(evt).await?;
            }
            BridgeEvent::GameStart(evt) => {
                self.handle_game_start(evt).await?;
            }
            BridgeEvent::MoveUpdate(evt) => {
                self.handle_move_update(evt).await?;
            }
            BridgeEvent::GameEnd(evt) => {
                tracing::info!("Game {} ended by {} ({})", evt.game_id, evt.winner.unwrap_or_default(), evt.reason);
                if let Some(game) = &self.active_game {
                    if game.game_id == evt.game_id {
                        self.active_game = None;
                    }
                }
            }
        }
        Ok(())
    }

    async fn handle_challenge(&mut self, evt: ChallengeReceivedEvent) -> Result<()> {
        tracing::info!("Received challenge {} from {}", evt.challenge_id, evt.challenger_username);

        if self.active_game.is_some() {
            tracing::info!("Declining challenge {}, already in a game", evt.challenge_id);
            self.http_client.decline_challenge(evt.challenge_id).await?;
            return Ok(());
        }

        let mins = evt.timer_settings.minutes_per_player as u64;
        let inc = evt.timer_settings.increment_seconds as i64;
        let ms_time = mins * 60 * 1000;
        let ms_inc = inc * 1000;

        // Find matching rule
        let matched_rule = self.config_rules.iter().find(|rule| rule.time == ms_time && rule.incr == ms_inc);

        if let Some(rule) = matched_rule {
            tracing::info!("Accepting challenge {} (matches rule)", evt.challenge_id);

            // We temporarily store the config rule so we know how to spawn the engine
            // However, gameStart event doesn't provide the challenge ID.
            // So we'll accept it and store it as pending.

            match self.http_client.accept_challenge(evt.challenge_id).await {
                Ok(_) => {
                    // Start partial state
                    let (engine_tx, engine_rx) = mpsc::channel(100);

                    // We spawn the engine immediately because we'll need to configure it
                    let mut engine =
                        EngineWrapper::spawn(&rule.engine_cmd, &rule.engine_args, engine_tx.clone()).await?;
                    if let Some(set) = &rule.engine_set {
                        engine.send_set(set).await?;
                    }

                    self.active_game = Some(GameState {
                        game_id: String::new(), // to be filled in gameStart
                        engine,
                        my_color: String::new(),
                        engine_tx,
                        engine_rx,
                        pending_challenge_id: Some(evt.challenge_id),
                        config_rule: rule.clone(),
                        last_move_time: None,
                    });
                }
                Err(e) => {
                    tracing::error!("Failed to accept challenge: {}", e);
                }
            }
        } else {
            tracing::info!("Declining challenge {} (no matching config rule)", evt.challenge_id);
            self.http_client.decline_challenge(evt.challenge_id).await?;
        }

        Ok(())
    }

    async fn handle_game_start(&mut self, evt: GameStartEvent) -> Result<()> {
        tracing::info!("Game {} starting against {}", evt.game_id, evt.opponent_name);

        if let Some(game) = self.active_game.as_mut() {
            game.game_id = evt.game_id.clone();
            game.my_color = evt.player_color.clone();

            // Ensure engine is ready before we process any moves
            game.engine.send_sync().await?;
            tracing::info!("Sent sync to engine, waiting for ready..");

            // Note: we're technically supposed to wait for the ready event, which will be handled in handle_engine_event
            // the site automatically pushes the first MoveUpdate if it's our turn eventually, or the opponent makes a move.
        } else {
            tracing::warn!("Received gameStart but we have no pending challenge state");
        }
        Ok(())
    }

    async fn handle_move_update(&mut self, evt: MoveUpdateEvent) -> Result<()> {
        if let Some(game) = self.active_game.as_mut() {
            if evt.game_over {
                tracing::info!("Move update indicates game over.");
                return Ok(());
            }

            // Reconstruct the squares notation. Row 0 is top, 7 is bottom in UI
            // Assuming standard a1-h8 notation:
            // clms 0-7 = a-h, rows 0-7 = 8-1
            let col_char = (b'a' + evt.col) as char;
            let row_num = 8 - evt.row;
            let sq = format!("{}{}", col_char, row_num);

            tracing::info!("Move update: {} by {}", sq, evt.player);

            // Only send the play if it wasn't us driving the move update
            // Actually, we should send all moves via `play` to keep state synced.
            game.engine.send_play(&[sq]).await?;

            // Check if it's our turn now
            if evt.current_player == game.my_color {
                let ms_left = if game.my_color == "white" { evt.timers.white } else { evt.timers.black } * 1000;

                let mut real_incr = game.config_rule.incr;

                // Subtract latency
                if let Some(start) = game.last_move_time {
                    let elapsed = start.elapsed().as_millis() as i64;
                    real_incr -= elapsed;
                }

                let go = GoCommand {
                    time: None,
                    nodes: None,
                    depth: None,
                    clock_left: Some(ms_left as u64),
                    clock_incr: Some(real_incr),
                };

                tracing::info!("Our turn! Sending go: {:?}", go);
                game.engine.send_go(&go).await?;
            }
        }

        Ok(())
    }

    async fn handle_engine_event(&mut self, event: EngineEvent) -> Result<()> {
        match event {
            EngineEvent::Best(Some(mv)) => {
                if let Some(game) = self.active_game.as_mut() {
                    // convert back from a1 to row/col
                    let mut chars = mv.chars();
                    if let (Some(c), Some(r)) = (chars.next(), chars.next()) {
                        let col = (c as u8) - b'a';
                        let row = 8 - (r as u8 - b'0');

                        tracing::info!("Engine chose best move {}, sending to site", mv);
                        game.last_move_time = Some(Instant::now());
                        self.socket_client.make_move(&game.game_id, row, col).await?;
                    }
                }
            }
            EngineEvent::Best(None) => {
                tracing::warn!("Engine returned best move: null (likely means it thinks it's over or invalid state)");
            }
            EngineEvent::Ready => {
                tracing::info!("Engine is ready");
                // if it's our turn at start, we might need to send go.
                // But the site usually sends a moveUpdate or we parse the gameStart board.
                // For this implementation, we rely on MoveUpdate triggering the first go if opponent moves,
                // or we need to handle if we are white and the game starts.

                // Let's improve game_start later to trigger Go if we are white and board is empty.
            }
            EngineEvent::Info(info) => {
                tracing::debug!("Engine info: {}", info);
            }
            EngineEvent::About(_) => {}
            EngineEvent::Warn(w) => tracing::warn!("Engine warning: {}", w),
            EngineEvent::Error(e) => tracing::error!("Engine error: {}", e),
        }

        Ok(())
    }
}
