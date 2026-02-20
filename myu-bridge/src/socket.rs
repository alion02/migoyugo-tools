use anyhow::{Context, Result};
use rust_socketio::asynchronous::{Client, ClientBuilder};
use tokio::sync::mpsc;

use crate::models::{
    ChallengeReceivedEvent, GameEndEvent, GameStartEvent, MakeMovePayload, MoveUpdateEvent, RematchRequestedEvent,
    RespondToRematchPayload,
};

#[derive(Debug)]
pub enum BridgeEvent {
    ChallengeReceived(ChallengeReceivedEvent),
    GameStart(GameStartEvent),
    MoveUpdate(MoveUpdateEvent),
    GameEnd(GameEndEvent),
    RematchRequested(RematchRequestedEvent),
    RematchAccepted(GameStartEvent),
    Connected,
    Disconnected,
    Pong,
}

pub struct MigoyugoSocketClient {
    client: Client,
}

impl MigoyugoSocketClient {
    pub async fn connect(base_url: &str, token: &str, event_tx: mpsc::Sender<BridgeEvent>) -> Result<Self> {
        let auth_payload = serde_json::json!({
            "token": token
        });

        let mut builder = ClientBuilder::new(base_url).auth(auth_payload);

        builder = builder.on_any(move |event, payload, _| {
            Box::pin(async move {
                tracing::debug!("RAW SOCKET EVENT: {:?} | PAYLOAD: {:?}", event, payload);
            })
        });

        let tx_clone = event_tx.clone();
        builder = builder.on("connect", move |_payload, _| {
            let tx = tx_clone.clone();
            Box::pin(async move {
                let _ = tx.send(BridgeEvent::Connected).await;
            })
        });

        let tx_clone = event_tx.clone();
        builder = builder.on("disconnect", move |_payload, _| {
            let tx = tx_clone.clone();
            Box::pin(async move {
                let _ = tx.send(BridgeEvent::Disconnected).await;
            })
        });

        let tx_clone = event_tx.clone();
        builder = builder.on("message", move |payload, _| {
            let tx = tx_clone.clone();
            Box::pin(async move {
                if let rust_socketio::Payload::Text(s) = payload
                    && let Some(msg) = s.first()
                    && msg.as_str() == Some("pong")
                {
                    let _ = tx.send(BridgeEvent::Pong).await;
                }
            })
        });

        let tx_clone = event_tx.clone();
        builder = builder.on("challengeReceived", move |payload, _| {
            let tx = tx_clone.clone();
            Box::pin(async move {
                if let rust_socketio::Payload::Text(s) = payload {
                    match serde_json::from_str::<ChallengeReceivedEvent>(&s[0].to_string()) {
                        Ok(event) => {
                            let _ = tx.send(BridgeEvent::ChallengeReceived(event)).await;
                        }
                        Err(e) => tracing::warn!("Failed to deserialize challengeReceived: {} (payload: {:?})", e, s),
                    }
                }
            })
        });

        let tx_clone = event_tx.clone();
        builder = builder.on("gameStart", move |payload, _| {
            let tx = tx_clone.clone();
            Box::pin(async move {
                if let rust_socketio::Payload::Text(s) = payload {
                    match serde_json::from_str::<GameStartEvent>(&s[0].to_string()) {
                        Ok(event) => {
                            let _ = tx.send(BridgeEvent::GameStart(event)).await;
                        }
                        Err(e) => tracing::warn!("Failed to deserialize gameStart: {} (payload: {:?})", e, s),
                    }
                } else {
                    tracing::warn!("gameStart payload was NOT text: {:?}", payload);
                }
            })
        });

        let tx_clone = event_tx.clone();
        builder = builder.on("moveUpdate", move |payload, _| {
            let tx = tx_clone.clone();
            Box::pin(async move {
                if let rust_socketio::Payload::Text(s) = payload {
                    match serde_json::from_str::<MoveUpdateEvent>(&s[0].to_string()) {
                        Ok(event) => {
                            let _ = tx.send(BridgeEvent::MoveUpdate(event)).await;
                        }
                        Err(e) => tracing::warn!("Failed to deserialize moveUpdate: {} (payload: {:?})", e, s),
                    }
                }
            })
        });

        let tx_clone = event_tx.clone();
        builder = builder.on("gameEnd", move |payload, _| {
            let tx = tx_clone.clone();
            Box::pin(async move {
                if let rust_socketio::Payload::Text(s) = payload {
                    match serde_json::from_str::<GameEndEvent>(&s[0].to_string()) {
                        Ok(event) => {
                            let _ = tx.send(BridgeEvent::GameEnd(event)).await;
                        }
                        Err(e) => tracing::warn!("Failed to deserialize gameEnd: {} (payload: {:?})", e, s),
                    }
                }
            })
        });

        let tx_clone = event_tx.clone();
        builder = builder.on("rematchRequested", move |payload, _| {
            let tx = tx_clone.clone();
            Box::pin(async move {
                if let rust_socketio::Payload::Text(s) = payload {
                    match serde_json::from_str::<RematchRequestedEvent>(&s[0].to_string()) {
                        Ok(event) => {
                            let _ = tx.send(BridgeEvent::RematchRequested(event)).await;
                        }
                        Err(e) => tracing::warn!("Failed to deserialize rematchRequested: {} (payload: {:?})", e, s),
                    }
                }
            })
        });

        let tx_clone = event_tx.clone();
        builder = builder.on("rematchAccepted", move |payload, _| {
            let tx = tx_clone.clone();
            Box::pin(async move {
                if let rust_socketio::Payload::Text(s) = payload {
                    match serde_json::from_str::<GameStartEvent>(&s[0].to_string()) {
                        Ok(event) => {
                            let _ = tx.send(BridgeEvent::RematchAccepted(event)).await;
                        }
                        Err(e) => tracing::warn!("Failed to deserialize rematchAccepted: {} (payload: {:?})", e, s),
                    }
                }
            })
        });

        let client = builder.connect().await.context("Failed to connect Socket.IO client")?;

        Ok(Self { client })
    }

    pub async fn make_move(&self, game_id: &str, row: u8, col: u8) -> Result<()> {
        let payload = MakeMovePayload { game_id: game_id.to_string(), row, col };

        let json_val = serde_json::to_value(payload).context("Failed to serialize move payload")?;
        self.client.emit("makeMove", json_val).await.context("Failed to emit makeMove")?;
        Ok(())
    }

    pub async fn respond_to_rematch(&self, game_id: &str, accept: bool) -> Result<()> {
        let payload = RespondToRematchPayload { game_id: game_id.to_string(), accept };
        let json_val = serde_json::to_value(payload).context("Failed to serialize respond to rematch payload")?;
        self.client.emit("respondToRematch", json_val).await.context("Failed to emit respondToRematch")?;
        Ok(())
    }

    pub async fn emit_ping(&self) -> Result<()> {
        self.client.emit("ping", serde_json::json!({})).await.context("Failed to emit ping")?;
        Ok(())
    }

    pub async fn disconnect(&self) -> Result<()> {
        self.client.disconnect().await.context("Failed to disconnect Socket.IO client")?;
        Ok(())
    }
}
