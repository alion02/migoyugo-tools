use anyhow::{Context, Result};
use kdl::KdlDocument;

#[derive(Debug, Clone)]
pub struct BridgeConfig {
    pub accept_rules: Vec<AcceptRule>,
}

#[derive(Debug, Clone)]
pub struct AcceptRule {
    pub times: Vec<u64>,
    pub incrs: Vec<i64>,
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
            let mut times = Vec::new();
            let mut incrs = Vec::new();
            let mut engine_cmd = None;
            let mut engine_args = Vec::new();
            let mut engine_set = None;

            if let Some(children) = node.children() {
                for child in children.nodes() {
                    match child.name().value() {
                        "time" => {
                            for entry in child.entries() {
                                if let Some(val) = entry.value().as_integer() {
                                    times.push(val as u64);
                                }
                            }
                        }
                        "incr" => {
                            for entry in child.entries() {
                                if let Some(val) = entry.value().as_integer() {
                                    incrs.push(val as i64);
                                }
                            }
                        }
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

            if times.is_empty() || incrs.is_empty() {
                anyhow::bail!("Missing or empty 'time' / 'incr' properties in 'accept' node");
            }

            let engine_cmd = engine_cmd.context("Missing 'engine' block in 'accept' node")?;

            accept_rules.push(AcceptRule { times, incrs, engine_cmd, engine_args, engine_set });
        }
    }

    Ok(BridgeConfig { accept_rules })
}
