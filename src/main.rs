#![feature(portable_simd)]
#![allow(
    clippy::missing_transmute_annotations, // don't care
)]

use std::{
    io::stdin,
    panic::{AssertUnwindSafe, catch_unwind, panic_any},
    sync::{Arc, atomic, mpsc::channel},
    thread::spawn,
    time::Instant,
};

use multiptr::MultiMut;

use crate::{
    protocol::{EngineMsg, Mv, UserMsg, send, send_error},
    search::ExitSearch,
    state::{Frame, Global, MakeResult, apply, make},
};

pub mod protocol;
pub mod search;
pub mod state;

#[derive(Debug, Clone)]
struct Position {
    stack: Vec<Frame>,
    index: usize,
    unplayable: bool,
}

impl Position {
    fn frame_ptr(&mut self) -> MultiMut<'_, Frame> {
        unsafe { MultiMut::from_slice_index(&mut self.stack, self.index) }
    }
}

impl Default for Position {
    fn default() -> Self {
        Self {
            stack: (-1..416).map(|ply| Frame { opp_migo: 0, opp_yugo: 0, score: 0, ply }).collect(),
            index: 1,
            unplayable: false,
        }
    }
}

struct Search {
    position: Position,
    global: Arc<Global>,
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
    let (search_tx, search_rx) = channel();
    spawn(move || {
        for Search { mut position, global } in search_rx {
            let f = position.frame_ptr();
            let thread = &mut Default::default();
            let mut best = None;
            for depth in 1.. {
                let result = catch_unwind(AssertUnwindSafe(|| search::search(&global, thread, f, depth)));
                match result {
                    Ok((eval, mv)) => {
                        best = Mv::new(mv);
                        send(&EngineMsg::Info {
                            pv: vec![best.unwrap()],
                            eval,
                            depth,
                            nodes: thread.nodes,
                            time: global.elapsed(),
                        });
                    }
                    Err(e) => {
                        if e.downcast_ref::<ExitSearch>().is_none() {
                            panic_any(e);
                        }
                        break;
                    }
                }
            }
            send(&EngineMsg::Best(best));
        }
    });
    let mut position = Position::default();
    let mut global: Option<Arc<Global>> = None;
    loop {
        let stop = || {
            if let Some(global) = &global {
                global.stop.store(true, atomic::Ordering::Relaxed);
            }
        };
        match ron::from_str::<UserMsg>(&msg) {
            Ok(msg) => match msg {
                UserMsg::Reset => position = Position::default(),
                UserMsg::Sync => send(&EngineMsg::Ready),
                UserMsg::State { undo, play } => 'b: {
                    let Some(new_index) = position.index.checked_sub(undo).filter(|index| index >= &1) else {
                        send_error("Too many undos");
                        break 'b;
                    };
                    if undo > 0 {
                        position.unplayable = false;
                    }
                    position.index = new_index;
                    for mv in play {
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
                    }
                }
                UserMsg::Go { node, ms } => 'b: {
                    if position.unplayable {
                        send_error("Cannot search this game state");
                        break 'b;
                    }
                    let new_global = Arc::new(Global {
                        started_at: Instant::now(),
                        stop: false.into(),
                        node_limits: node,
                        ms_limits: ms,
                    });
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
