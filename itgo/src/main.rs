#![feature(portable_simd)]
#![allow(
    clippy::missing_transmute_annotations, // don't care
    clippy::collapsible_if, // less noisy to update
)]

use std::{
    borrow::Cow,
    io::stdin,
    sync::{Arc, atomic, mpsc::channel},
    thread::spawn,
    time::Instant,
};

use myu_protocol::{EngineMsg, UserMsg, deserialize, serialize};

use crate::{engine::Cmd, state::Global};

pub mod engine;
pub mod search;
pub mod state;

fn main() {
    send(&EngineMsg::Id { name: Some("Itgo".into()), author: None, version: None });
    let (line_tx, line_rx) = channel();
    spawn(move || {
        for line in stdin().lines().map_while(Result::ok) {
            if line_tx.send(line).is_err() {
                break;
            }
        }
    });
    let Ok(mut msg) = line_rx.recv() else { return };
    let cmd_tx = engine::start();
    let mut global: Option<Arc<Global>> = None;
    loop {
        let stop = || {
            let Some(global) = &global else { return };
            global.stop.store(true, atomic::Ordering::Relaxed);
        };
        let forward = |msg| cmd_tx.send(Cmd::Msg(msg)).unwrap();
        match deserialize(&msg) {
            Ok(msg) => match msg {
                UserMsg::Reset | UserMsg::Undo(_) | UserMsg::Play(_) | UserMsg::Debug => {
                    stop();
                    forward(msg);
                }
                UserMsg::Sync => {
                    if global.as_ref().is_none_or(|global| global.stop.load(atomic::Ordering::Relaxed)) {
                        // engine thread is not searching or stopping search right now, but it might be doing other things
                        // rendezvous with it (0 capacity channel enforces synchronous handshake behavior)
                        forward(msg);
                    }
                    send(&EngineMsg::Ready);
                }
                UserMsg::Go(limits) => {
                    let new_global = Arc::new(Global { started_at: Instant::now(), stop: false.into(), limits });
                    stop();
                    global = Some(new_global.clone());
                    cmd_tx.send(Cmd::Start(new_global)).unwrap();
                }
                UserMsg::Stop => stop(),
            },
            Err(e) => send_error(e.to_string()),
        }
        let Ok(next) = line_rx.recv() else { return };
        msg = next;
    }
}

fn send(msg: &EngineMsg) {
    println!("{}", serialize(msg).unwrap());
}

fn send_error(error: impl Into<Cow<'static, str>>) {
    send(&EngineMsg::Error(error.into()));
}
