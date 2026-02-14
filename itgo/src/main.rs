#![feature(portable_simd)]
#![allow(
    clippy::missing_transmute_annotations, // don't care
    clippy::collapsible_if, // less noisy to update
    static_mut_refs, // actively harmful
)]

pub mod game;
pub mod protocol;
pub mod search;
pub mod searcher;
pub mod shared;
pub mod state;
pub mod thread;
pub mod tt;
pub mod util;

use std::{
    borrow::Cow,
    io::stdin,
    sync::{Arc, mpsc::channel},
    thread::spawn,
    time::Instant,
};

use parking_lot::RwLock;

use crate::{
    protocol::{
        EngineMsg, UserMsg, deserialize, serialize,
        settings::{BlockingCommand, Settings},
    },
    searcher::Cmd,
    shared::Shared,
};

fn main() {
    send(&EngineMsg::About {
        name: "Itgo",
        author: "alion02",
        version: env!("VERGEN_GIT_DESCRIBE"),
        features: &["interactive", "fixed_time", "fixed_nodes", "fixed_depth", "dyn_mem"],
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
    tt::init_hash();
    let mut settings = Settings::default();
    let shared = Arc::new(RwLock::new(Shared::new(settings.tt_len())));
    let cmd_tx = searcher::start();
    loop {
        let stop = || shared.read().set_active(false);
        let blocking_command = settings.blocking_command;
        let handle_active = || {
            if shared.read().active() {
                match blocking_command {
                    BlockingCommand::Warn => {
                        send_warn(
                            "Received blocking command while searching, waiting until search naturally finishes; \
                             no commands will be processed until then",
                        );
                    }
                    BlockingCommand::Allow => (),
                    BlockingCommand::Stop => stop(),
                }
            }
        };
        match deserialize(&msg) {
            Ok(msg) => match msg {
                UserMsg::Set(patch) => {
                    for (key, value) in patch.unknown.as_object().unwrap().into_iter() {
                        send_warn(format!("Tried to set unknown key `{key}` to `{value}`"));
                    }
                    settings.apply(&patch);
                    let target_len = settings.tt_len();
                    if target_len != shared.read().tt.raw.len() {
                        handle_active();
                        shared.write().tt.resize(target_len);
                    }
                }
                UserMsg::Play(mvs) => {
                    handle_active();
                    if let Err(e) = shared.write().game.play(&mvs, false) {
                        send_error(e);
                    }
                }
                UserMsg::Undo(count) => {
                    handle_active();
                    if let Err(e) = shared.write().game.undo(count) {
                        send_error(e);
                    }
                }
                UserMsg::Moves(mvs) => {
                    handle_active();
                    if let Err(e) = shared.write().game.play(&mvs, true) {
                        send_error(e);
                    }
                }
                // TODO: Pos
                UserMsg::Reset => {
                    handle_active();
                    cmd_tx.send(Cmd::Reset).unwrap();
                    shared.write().reset();
                }
                UserMsg::Sync => {
                    if shared.read().active() {
                        // searcher thread is not searching right now, but it might be doing other things
                        // rendezvous with it (0 capacity channel enforces synchronous handshake behavior)
                        cmd_tx.send(Cmd::Sync).unwrap();
                    }
                    send(&EngineMsg::Ready);
                }
                UserMsg::Go(limits) => {
                    // TODO: handle terminal states, requires minor refactor of terminal checking
                    handle_active();
                    shared.write().go(Instant::now(), limits);
                    cmd_tx.send(Cmd::Go { shared: shared.read_arc() }).unwrap();
                }
                UserMsg::Stop => {
                    stop();
                }
                UserMsg::Debug => {
                    eprintln!("{settings:#?}");
                    handle_active();
                    cmd_tx.send(Cmd::Debug).unwrap();
                }
            },
            Err(e) => send_error(e.to_string()),
        }
        let Ok(next) = line_rx.recv() else { break };
        msg = next;
    }
    cmd_tx.send(Cmd::Sync).unwrap();
}

fn send(msg: &EngineMsg) {
    println!("{}", serialize(msg).unwrap());
}

fn send_warn(warn: impl Into<Cow<'static, str>>) {
    send(&EngineMsg::Warn(warn.into()));
}

fn send_error(error: impl Into<Cow<'static, str>>) {
    send(&EngineMsg::Error(error.into()));
}
