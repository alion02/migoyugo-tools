#![feature(portable_simd)]
#![allow(
    clippy::missing_transmute_annotations, // don't care
)]

use std::{io::stdin, sync::mpsc::channel, thread::spawn};

use multiptr::MultiMut;

use crate::{
    protocol::{EngineMsg, UserMsg, send, send_error},
    state::{Frame, MakeResult, apply, make},
};

pub mod protocol;
pub mod search;
pub mod state;

struct Position {
    stack: Vec<Frame>,
    index: usize,
    unplayable: bool,
}

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
    let (search_tx, search_rx) = channel::<()>();
    spawn(move || for search in search_rx {});
    let mut position = None;
    loop {
        match ron::from_str::<UserMsg>(&msg) {
            Ok(msg) => match msg {
                UserMsg::New => {
                    position = Some(Position {
                        stack: (-1..416).map(|ply| Frame { opp_migo: 0, opp_yugo: 0, score: 0, ply }).collect(),
                        index: 1,
                        unplayable: false,
                    })
                }
                UserMsg::Sync(id) => send(&EngineMsg::Ready(id)),
                UserMsg::State { undo, play } => 'b: {
                    let Some(ref mut position) = position else {
                        send_error("No position to update state on.");
                        break 'b;
                    };
                    let Some(new_index) = position.index.checked_sub(undo).filter(|index| index >= &1) else {
                        send_error("Too many undos.");
                        break 'b;
                    };
                    if undo > 0 {
                        position.unplayable = false;
                    }
                    position.index = new_index;
                    for mv in play {
                        if position.unplayable {
                            send_error("Cannot play on this game state.");
                            break 'b;
                        }
                        let f = unsafe { MultiMut::from_slice_index(&mut position.stack, position.index) };
                        match make(f, mv.raw()).ok() {
                            Some(data) => apply(f.offset(1), data),
                            None => position.unplayable = true, // FIXME: kinda weird that illegal and legal moves are treated the same
                        }
                    }
                }
                UserMsg::Go { node, time } => todo!(),
            },
            Err(e) => send_error(e.to_string()),
        }
        let Ok(next) = line_rx.recv() else { return };
        msg = next;
    }
}
