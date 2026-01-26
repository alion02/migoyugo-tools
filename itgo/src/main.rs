#![feature(portable_simd)]
#![allow(
    clippy::missing_transmute_annotations, // don't care
    clippy::collapsible_if, // less noisy to update
)]

use std::{
    borrow::Cow,
    io::stdin,
    sync::{Arc, RwLock, atomic, mpsc::channel},
    thread::spawn,
    time::Instant,
};

use myu_protocol::{EngineMsg, UserMsg, deserialize, serialize};

use crate::{
    engine::Cmd,
    limits::Limits,
    options::{BlockingCommand, Options},
    shared::Shared,
    state::Global,
};

pub mod engine;
pub mod game;
pub mod limits;
pub mod options;
pub mod search;
pub mod shared;
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
    let shared = Arc::<RwLock<Shared>>::default();
    let mut options = Options::default();
    loop {
        let stop = || shared.read().unwrap().set_active(false);
        let handle_active = || {
            if shared.read().unwrap().active() {
                match options.blocking_command {
                    BlockingCommand::Warn => {
                        send_warn("Received blocking command while searching, waiting until search naturally finishes");
                    }
                    BlockingCommand::Allow => (),
                    BlockingCommand::Stop => stop(),
                }
            }
        };
        match deserialize(&msg) {
            Ok(msg) => match msg {
                UserMsg::Play(mvs) => {
                    handle_active();
                    shared.write().unwrap().game.play(&mvs);
                }
                UserMsg::Undo(count) => {
                    handle_active();
                    shared.write().unwrap().game.undo(count);
                }
                // TODO: Moves, Pos
                UserMsg::Reset => {
                    handle_active();
                    cmd_tx.send(Cmd::Reset).unwrap();
                    shared.write().unwrap().game.reset();
                }
                UserMsg::Sync => {
                    if shared.read().unwrap().active() {
                        // engine thread is not searching right now, but it might be doing other things
                        // rendezvous with it (0 capacity channel enforces synchronous handshake behavior)
                        cmd_tx.send(Cmd::Sync);
                    }
                    send(&EngineMsg::Ready);
                }
                UserMsg::Go(limits) => {
                    handle_active();
                    shared.write().unwrap().go(Instant::now(), Limits::new(limits));
                    cmd_tx.send(Cmd::Go).unwrap();
                }
                UserMsg::Stop => {
                    stop();
                }
                UserMsg::Debug => {
                    handle_active();
                    cmd_tx.send(Cmd::Debug).unwrap();
                }
            },
            Err(e) => send_error(e.to_string()),
        }
        let Ok(next) = line_rx.recv() else { return };
        msg = next;
    }
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

fn send_warn(warn: impl Into<Cow<'static, str>>) {
    // TODO: ::Warn
    send(&EngineMsg::Error(warn.into()));
}
