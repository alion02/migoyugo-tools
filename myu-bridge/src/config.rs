use anyhow::{Context, Result};
use kdl::KdlDocument;

#[derive(Debug, Clone)]
pub struct BridgeConfig {
    pub accept_rules: Vec<AcceptRule>,
}

#[derive(Debug, Clone)]
pub struct AcceptRule {
    pub time: u64,
    pub incr: i64,
    pub engine_cmd: String,
    pub engine_args: Vec<String>,
    pub engine_set: Option<serde_json::Value>,
}

pub fn parse_config(path: &std::path::Path) -> Result<BridgeConfig> {
    let content = std::fs::read_to_string(path).with_context(|| format!("Failed to read config file at {:?}", path))?;

    let doc: KdlDocument = content.parse().context("Failed to parse KDL config")?;
    let mut accept_rules = Vec::new();

    for node in doc.nodes() {
        if node.name().value() == "accept" {
            let time = node
                .get("time")
                .and_then(|e| e.as_integer())
                .context("Missing or invalid 'time' property in 'accept' node")? as u64;

            let incr = node
                .get("incr")
                .and_then(|e| e.as_integer())
                .context("Missing or invalid 'incr' property in 'accept' node")? as i64;

            let mut engine_cmd = None;
            let mut engine_args = Vec::new();
            let mut engine_set = None;

            if let Some(children) = node.children() {
                for child in children.nodes() {
                    match child.name().value() {
                        "engine" => {
                            for (i, entry) in child.entries().iter().enumerate() {
                                if let Some(val) = entry.value().as_string() {
                                    if i == 0 {
                                        engine_cmd = Some(val.to_string());
                                    } else {
                                        engine_args.push(val.to_string());
                                    }
                                }
                            }
                        }
                        "set" => {
                            if let Some(val) = child.get(0).and_then(|e| e.as_string()) {
                                let parsed: serde_json::Value = serde_json::from_str(val)
                                    .with_context(|| format!("Failed to parse 'set' json payload: {}", val))?;
                                engine_set = Some(parsed);
                            }
                        }
                        _ => {}
                    }
                }
            }

            let engine_cmd = engine_cmd.context("Missing 'engine' block in 'accept' node")?;

            accept_rules.push(AcceptRule { time, incr, engine_cmd, engine_args, engine_set });
        }
    }

    Ok(BridgeConfig { accept_rules })
}
