use std::future;
use std::sync::Arc;

use anyhow::Result;
use tokio::sync::mpsc;
use tokio::time::{Duration, Instant};

use crate::config::AcceptRule;
use crate::engine::{Clock, EngineEvent, EngineWrapper, GoCommand};
use crate::http::MigoyugoHttpClient;
use crate::models::{ChallengeReceivedEvent, GameStartEvent, MoveUpdateEvent, RematchRequestedEvent};
use crate::socket::{BridgeEvent, MigoyugoSocketClient};

pub struct LastGameInfo {
    pub rule: AcceptRule,
    pub matched_time_ms: u64,
    pub matched_incr_ms: i64,
}

pub struct GameState {
    pub game_id: String,
    pub engine: EngineWrapper,
    pub my_color: String,
    pub engine_tx: mpsc::Sender<EngineEvent>,
    pub engine_rx: mpsc::Receiver<EngineEvent>,
    pub pending_challenge_id: Option<u64>,
    pub config_rule: AcceptRule,
    pub matched_incr_ms: i64,
    pub matched_time_ms: u64,
    pub moves_played: u32,
}

impl GameState {
    pub async fn trigger_go(&mut self, ms_left: u64, average_latency_ms: u64) -> Result<()> {
        const TIME_BUFFER: u64 = 2000;
        let mut real_left = ms_left;
        let mut real_incr = self.matched_incr_ms;

        real_left = real_left.saturating_sub(TIME_BUFFER + average_latency_ms);
        real_incr -= average_latency_ms as i64;

        let go =
            GoCommand { time: None, nodes: None, depth: None, clock: Some(Clock { left: real_left, incr: real_incr }) };

        tracing::info!("Our turn! Sending go: {:?}", go);
        self.engine.send_go(&go).await
    }
}

pub struct Controller {
    pub http_client: Arc<MigoyugoHttpClient>,
    pub socket_client: Arc<MigoyugoSocketClient>,
    pub config_path: std::path::PathBuf,
    pub config_rules: Vec<AcceptRule>,
    pub socket_rx: mpsc::Receiver<BridgeEvent>,

    // Simplification for this bridge: handle one active game at a time.
    pub active_game: Option<GameState>,
    pub last_game_info: Option<LastGameInfo>,

    pub latency_samples: std::collections::VecDeque<u64>,
    pub last_ping_sent: Option<Instant>,
    pub shutting_down: bool,
}

impl Controller {
    pub fn new(
        http_client: Arc<MigoyugoHttpClient>,
        socket_client: Arc<MigoyugoSocketClient>,
        config_path: std::path::PathBuf,
        config_rules: Vec<AcceptRule>,
        socket_rx: mpsc::Receiver<BridgeEvent>,
    ) -> Self {
        Self {
            http_client,
            socket_client,
            config_path,
            config_rules,
            socket_rx,
            active_game: None,
            last_game_info: None,
            latency_samples: std::collections::VecDeque::new(),
            last_ping_sent: None,
            shutting_down: false,
        }
    }

    pub fn average_latency(&self) -> u64 {
        if self.latency_samples.is_empty() {
            // Conservative default
            return 500;
        }
        let sum: u64 = self.latency_samples.iter().sum();
        sum / (self.latency_samples.len() as u64)
    }

    pub async fn run(&mut self) -> Result<()> {
        const PING_INTERVAL: Duration = Duration::from_secs(3);
        // Delay first ping to try and wait for upgrade to WebSockets
        let mut ping_interval = tokio::time::interval_at(Instant::now() + PING_INTERVAL, PING_INTERVAL);

        let mut config_interval = tokio::time::interval(Duration::from_secs(1));
        let mut last_config_mod = std::fs::metadata(&self.config_path)
            .and_then(|m| m.modified())
            .unwrap_or_else(|_| std::time::SystemTime::now());

        let mut ctrl_c = std::pin::pin!(tokio::signal::ctrl_c());

        loop {
            if self.shutting_down && self.active_game.is_none() {
                tracing::info!("No active game and shutting down gracefully. Exiting loop.");
                break;
            }

            tokio::select! {
                res = &mut ctrl_c, if !self.shutting_down => {
                    match res {
                        Ok(()) => {
                            tracing::info!("Ctrl-C received! Refusing new games and shutting down when current game finishes.");
                            self.shutting_down = true;
                        }
                        Err(e) => {
                            tracing::error!("Failed to listen for ctrl-c: {}", e);
                        }
                    }
                }

                _ = config_interval.tick() => {
                    if let Ok(modified) = std::fs::metadata(&self.config_path).and_then(|m| m.modified())
                        && modified > last_config_mod
                    {
                        last_config_mod = modified;
                        match crate::config::parse_config(&self.config_path) {
                            Ok(cfg) => {
                                tracing::info!("Config file modified. Reloaded {} rules.", cfg.accept_rules.len());
                                self.config_rules = cfg.accept_rules;
                            }
                            Err(e) => {
                                tracing::error!("Failed to reload config: {}", e);
                            }
                        }
                    }
                }

                _ = ping_interval.tick() => {
                    if let Err(e) = self.socket_client.emit_ping().await {
                        tracing::error!("Failed to ping: {}", e);
                    } else {
                        self.last_ping_sent = Some(Instant::now());
                    }
                }

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
        Ok(())
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
                if let Some(game) = &self.active_game
                    && game.game_id == evt.game_id
                {
                    self.last_game_info = Some(LastGameInfo {
                        rule: game.config_rule.clone(),
                        matched_incr_ms: game.matched_incr_ms,
                        matched_time_ms: game.matched_time_ms,
                    });
                    self.active_game = None;
                }
            }
            BridgeEvent::RematchRequested(evt) => {
                self.handle_rematch_requested(evt).await?;
            }
            BridgeEvent::RematchAccepted(evt) => {
                self.handle_rematch_accepted(evt).await?;
            }
            BridgeEvent::Pong => {
                if let Some(start) = self.last_ping_sent.take() {
                    let latency = start.elapsed().as_millis() as u64;
                    if self.latency_samples.len() >= 10 {
                        self.latency_samples.pop_front();
                    }
                    self.latency_samples.push_back(latency);
                    tracing::debug!("Measured latency: {} ms (avg: {} ms)", latency, self.average_latency());
                }
            }
        }
        Ok(())
    }

    async fn handle_challenge(&mut self, evt: ChallengeReceivedEvent) -> Result<()> {
        tracing::info!("Received challenge {} from {}", evt.challenge_id, evt.challenger_username);

        if self.shutting_down {
            tracing::info!("Declining challenge {}, shutting down gracefully", evt.challenge_id);
            self.http_client.decline_challenge(evt.challenge_id).await?;
            return Ok(());
        }

        if self.active_game.is_some() {
            tracing::info!("Declining challenge {}, already in a game", evt.challenge_id);
            self.http_client.decline_challenge(evt.challenge_id).await?;
            return Ok(());
        }

        let mins = evt.timer_settings.minutes_per_player as u64;
        let inc = evt.timer_settings.increment_seconds as i64;

        let msg_time_sec = mins * 60;
        let msg_incr_sec = inc;

        // Find matching rule
        let matched_rule = self
            .config_rules
            .iter()
            .find(|rule| rule.times.contains(&msg_time_sec) && rule.incrs.contains(&msg_incr_sec));

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
                        matched_incr_ms: msg_incr_sec * 1000,
                        matched_time_ms: msg_time_sec * 1000,
                        moves_played: 0,
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
        tracing::info!("Game {} starting, as {}", evt.game_id, evt.player_color);

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

    async fn handle_rematch_requested(&mut self, evt: RematchRequestedEvent) -> Result<()> {
        tracing::info!("Rematch requested for game {} by {}", evt.game_id, evt.requester_name);

        if self.shutting_down {
            tracing::info!("Declining rematch request for game {} (shutting down)", evt.game_id);
            self.socket_client.respond_to_rematch(&evt.game_id, false).await?;
            return Ok(());
        }

        // We always accept if we have last game info and aren't in another game
        if self.last_game_info.is_some() && self.active_game.is_none() {
            tracing::info!("Accepting rematch request for game {}", evt.game_id);
            self.socket_client.respond_to_rematch(&evt.game_id, true).await?;
        } else {
            tracing::info!(
                "Declining rematch request for game {} (no previous game info or already in game)",
                evt.game_id
            );
            self.socket_client.respond_to_rematch(&evt.game_id, false).await?;
        }

        Ok(())
    }

    async fn handle_rematch_accepted(&mut self, evt: GameStartEvent) -> Result<()> {
        tracing::info!("Rematch accepted! New game {} starting, as {}", evt.game_id, evt.player_color);

        if self.active_game.is_some() {
            tracing::warn!("Already in a game, ignoring rematch accepted");
            return Ok(());
        }

        if let Some(info) = self.last_game_info.take() {
            let (engine_tx, engine_rx) = mpsc::channel(100);

            // Spawn the engine
            let mut engine =
                EngineWrapper::spawn(&info.rule.engine_cmd, &info.rule.engine_args, engine_tx.clone()).await?;
            if let Some(set) = &info.rule.engine_set {
                engine.send_set(set).await?;
            }

            self.active_game = Some(GameState {
                game_id: evt.game_id.clone(),
                engine,
                my_color: evt.player_color.clone(),
                engine_tx,
                engine_rx,
                pending_challenge_id: None,
                config_rule: info.rule,
                matched_incr_ms: info.matched_incr_ms,
                matched_time_ms: info.matched_time_ms,
                moves_played: 0,
            });

            // Ensure engine is ready before we process any moves
            self.active_game.as_mut().unwrap().engine.send_sync().await?;
            tracing::info!("Sent sync to engine, waiting for ready..");
        } else {
            tracing::warn!("Received rematchAccepted but have no last_game_info");
        }

        Ok(())
    }

    async fn handle_move_update(&mut self, evt: MoveUpdateEvent) -> Result<()> {
        let avg_latency = self.average_latency();
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
                let ms_left =
                    (if game.my_color == "white" { evt.timers.white } else { evt.timers.black } * 1000) as u64;
                game.trigger_go(ms_left, avg_latency).await?;
            }
        }

        Ok(())
    }

    async fn handle_engine_event(&mut self, event: EngineEvent) -> Result<()> {
        let avg_latency = self.average_latency();
        match event {
            EngineEvent::Best(Some(mv)) => {
                if let Some(game) = self.active_game.as_mut() {
                    // convert back from a1 to row/col
                    let mut chars = mv.chars();
                    if let (Some(c), Some(r)) = (chars.next(), chars.next()) {
                        let col = (c as u8) - b'a';
                        let row = 8 - (r as u8 - b'0');

                        tracing::info!("Engine chose best move {}, sending to site", mv);
                        game.moves_played += 1;
                        self.socket_client.make_move(&game.game_id, row, col).await?;
                    }
                }
            }
            EngineEvent::Best(None) => {
                tracing::warn!("Engine returned best move: null (likely means it thinks it's over or invalid state)");
            }
            EngineEvent::Ready => {
                tracing::info!("Engine is ready");
                if let Some(game) = self.active_game.as_mut() {
                    // If we are white and haven't played a move yet, send the initial go
                    if game.my_color == "white" && game.moves_played == 0 {
                        let ms_left = game.matched_time_ms;
                        game.trigger_go(ms_left, avg_latency).await?
                    }
                }
            }
            EngineEvent::Info(info) => {
                tracing::info!("Engine info: {}", info);
            }
            EngineEvent::About(_) => {}
            EngineEvent::Warn(w) => tracing::warn!("Engine warning: {}", w),
            EngineEvent::Error(e) => tracing::error!("Engine error: {}", e),
        }

        Ok(())
    }
}
