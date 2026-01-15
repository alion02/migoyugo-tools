#![feature(portable_simd)]
#![allow(
    clippy::missing_transmute_annotations, // don't care
)]

use std::{
    borrow::Cow,
    io::stdin,
    sync::{Arc, atomic, mpsc::channel},
    thread::spawn,
    time::Instant,
};

use myu_protocol::{EngineMsg, UserMsg, deserialize, serialize};

use crate::{
    engine::{Position, Search},
    state::{Global, MakeResult, apply, make},
};

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
    let search_tx = engine::start();
    let mut position = Position::default();
    let mut global: Option<Arc<Global>> = None;
    loop {
        let stop = || {
            if let Some(global) = &global {
                global.stop.store(true, atomic::Ordering::Relaxed);
            }
        };
        match deserialize(&msg) {
            Ok(msg) => match msg {
                UserMsg::Reset => position = Position::default(),
                UserMsg::Sync => send(&EngineMsg::Ready),
                UserMsg::Undo(count) => 'b: {
                    let Some(new_index) = position.index.checked_sub(count).filter(|index| index >= &1) else {
                        send_error("Too many undos");
                        break 'b;
                    };
                    if count > 0 {
                        position.unplayable = false;
                    }
                    position.index = new_index;
                }
                UserMsg::Play(mvs) => 'b: {
                    for mv in mvs {
                        if position.unplayable {
                            send_error("Cannot play on this game state");
                            break 'b;
                        }
                        let f = position.frame_ptr();
                        // TODO: is_legal, is_terminal
                        match make(f, mv.raw()) {
                            MakeResult::Ok(data) => apply(f.offset(1), data),
                            MakeResult::Illegal => todo!(),
                            MakeResult::Igo => position.unplayable = true,
                        }
                        position.index += 1;
                    }
                }
                UserMsg::Go(limits) => 'b: {
                    if position.unplayable {
                        send(&EngineMsg::Best(None));
                        send_error("Cannot search this game state");
                        break 'b;
                    }
                    let new_global = Arc::new(Global { started_at: Instant::now(), stop: false.into(), limits });
                    stop();
                    global = Some(new_global.clone());
                    search_tx.send(Search { position: position.clone(), global: new_global }).unwrap();
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
