use anyhow::{Context, Result};
use rust_socketio::asynchronous::{Client, ClientBuilder};
use tokio::sync::mpsc;

use crate::models::{ChallengeReceivedEvent, GameEndEvent, GameStartEvent, MakeMovePayload, MoveUpdateEvent};

#[derive(Debug)]
pub enum BridgeEvent {
    ChallengeReceived(ChallengeReceivedEvent),
    GameStart(GameStartEvent),
    MoveUpdate(MoveUpdateEvent),
    GameEnd(GameEndEvent),
    Connected,
    Disconnected,
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
                tracing::info!("RAW SOCKET EVENT: {:?} | PAYLOAD: {:?}", event, payload);
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

        let client = builder.connect().await.context("Failed to connect Socket.IO client")?;

        Ok(Self { client })
    }

    pub async fn make_move(&self, game_id: &str, row: u8, col: u8) -> Result<()> {
        let payload = MakeMovePayload { game_id: game_id.to_string(), row, col };

        let json_val = serde_json::to_value(payload).context("Failed to serialize move payload")?;
        self.client.emit("makeMove", json_val).await.context("Failed to emit makeMove")?;
        Ok(())
    }

    pub async fn disconnect(&self) -> Result<()> {
        self.client.disconnect().await.context("Failed to disconnect Socket.IO client")?;
        Ok(())
    }
}
