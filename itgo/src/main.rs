#![feature(portable_simd)]
#![allow(
    clippy::missing_transmute_annotations, // don't care
    clippy::collapsible_if, // less noisy to update
)]

pub mod game;
pub mod limits;
pub mod options;
pub mod protocol;
pub mod search;
pub mod searcher;
pub mod shared;
pub mod state;
pub mod thread;

use std::{
    borrow::Cow,
    io::stdin,
    sync::{Arc, RwLock, mpsc::channel},
    thread::spawn,
    time::Instant,
};

use crate::{
    limits::Limits,
    options::{BlockingCommand, Options},
    protocol::{EngineMsg, UserMsg, deserialize, serialize},
    searcher::Cmd,
    shared::Shared,
};

fn main() {
    send(&EngineMsg::About {
        name: "Itgo",
        author: "alion02",
        version: env!("VERGEN_GIT_DESCRIBE"),
        settings: &[],
        features: &[],
    });
    let (line_tx, line_rx) = channel();
    spawn(move || {
        for line in stdin().lines().map_while(Result::ok) {
            if line_tx.send(line).is_err() {
                break;
            }
        }
    });
    let Ok(mut msg) = line_rx.recv() else { return };
    let shared = Arc::<RwLock<Shared>>::default();
    let cmd_tx = searcher::start(shared.clone());
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
                    if let Err(e) = shared.write().unwrap().game.play(&mvs) {
                        send_error(e);
                    }
                }
                UserMsg::Undo(count) => {
                    handle_active();
                    if let Err(e) = shared.write().unwrap().game.undo(count) {
                        send_error(e);
                    }
                }
                // TODO: Moves, Pos
                UserMsg::Reset => {
                    handle_active();
                    cmd_tx.send(Cmd::Reset).unwrap();
                    shared.write().unwrap().game.reset();
                }
                UserMsg::Sync => {
                    if shared.read().unwrap().active() {
                        // searcher thread is not searching right now, but it might be doing other things
                        // rendezvous with it (0 capacity channel enforces synchronous handshake behavior)
                        cmd_tx.send(Cmd::Sync).unwrap();
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
