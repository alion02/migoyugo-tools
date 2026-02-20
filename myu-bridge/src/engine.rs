use std::process::Stdio;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::mpsc;

#[derive(Debug, Serialize, Deserialize)]
pub struct GoCommand {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depth: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nodes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clock_left: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clock_incr: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineEvent {
    Best(Option<String>),
    Ready,
    Info(Value),
    About(Value),
    Warn(String),
    Error(String),
}

pub struct EngineWrapper {
    stdin: ChildStdin,
    _child: Child, // Held to keep the process alive
}

impl EngineWrapper {
    pub async fn spawn(cmd: &str, args: &[String], event_tx: mpsc::Sender<EngineEvent>) -> Result<Self> {
        let mut child = Command::new(cmd)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .with_context(|| format!("Failed to spawn engine: {}", cmd))?;

        let stdin = child.stdin.take().context("Failed to open engine stdin")?;
        let stdout = child.stdout.take().context("Failed to open engine stdout")?;

        // Spawn a background task to read stdout
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                if let Ok(value) = serde_json::from_str::<Value>(&line) {
                    if let Some(obj) = value.as_object() {
                        if obj.contains_key("best") {
                            let best = obj.get("best").and_then(|v| v.as_str()).map(String::from);
                            let _ = event_tx.send(EngineEvent::Best(best)).await;
                        } else if obj.contains_key("info") {
                            let _ = event_tx.send(EngineEvent::Info(obj.get("info").unwrap().clone())).await;
                        } else if obj.contains_key("about") {
                            let _ = event_tx.send(EngineEvent::About(obj.get("about").unwrap().clone())).await;
                        } else if obj.contains_key("warn") {
                            let msg = obj.get("warn").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            let _ = event_tx.send(EngineEvent::Warn(msg)).await;
                        } else if obj.contains_key("error") {
                            let msg = obj.get("error").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            let _ = event_tx.send(EngineEvent::Error(msg)).await;
                        }
                    } else if let Some(s) = value.as_str() {
                        if s == "ready" {
                            let _ = event_tx.send(EngineEvent::Ready).await;
                        }
                    }
                }
            }
        });

        Ok(Self { stdin, _child: child })
    }

    async fn send_line(&mut self, line: &str) -> Result<()> {
        let mut msg = line.to_string();
        msg.push('\n');
        self.stdin.write_all(msg.as_bytes()).await.context("Failed to write to engine")?;
        self.stdin.flush().await.context("Failed to flush engine stdin")?;
        Ok(())
    }

    pub async fn send_set(&mut self, settings: &Value) -> Result<()> {
        let payload = serde_json::json!({ "set": settings });
        self.send_line(&payload.to_string()).await
    }

    pub async fn send_play(&mut self, moves: &[String]) -> Result<()> {
        let payload = serde_json::json!({ "play": moves });
        self.send_line(&payload.to_string()).await
    }

    pub async fn send_moves(&mut self, moves: &[String]) -> Result<()> {
        let payload = serde_json::json!({ "moves": moves });
        self.send_line(&payload.to_string()).await
    }

    pub async fn send_go(&mut self, go_cmd: &GoCommand) -> Result<()> {
        let payload = serde_json::json!({ "go": go_cmd });
        self.send_line(&payload.to_string()).await
    }

    pub async fn send_sync(&mut self) -> Result<()> {
        self.send_line("\"sync\"").await
    }

    pub async fn send_stop(&mut self) -> Result<()> {
        self.send_line("\"stop\"").await
    }

    pub async fn send_reset(&mut self) -> Result<()> {
        self.send_line("\"reset\"").await
    }
}
