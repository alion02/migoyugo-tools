use std::{
    panic::{AssertUnwindSafe, catch_unwind, panic_any},
    sync::{
        Arc,
        mpsc::{SyncSender, sync_channel},
    },
    thread::spawn,
};

use multiptr::MultiMut;
use myu_protocol::{EngineMsg, Eval, Limit, Sq};

use crate::{
    search::{ExitSearch, search},
    send,
    state::{Frame, Global, Thread},
};

#[derive(Debug, Clone)]
pub struct Position {
    pub stack: Vec<Frame>,
    pub index: usize,
    pub unplayable: bool,
}

impl Position {
    pub fn frame_ptr(&mut self) -> MultiMut<'_, Frame> {
        unsafe { MultiMut::from_slice_index(&mut self.stack, self.index) }
    }
}

impl Default for Position {
    fn default() -> Self {
        Self {
            stack: (-1..416).map(|ply| Frame { opp_migo: 0, opp_yugo: 0, score: 0, ply, killers: [0, 1] }).collect(),
            index: 1,
            unplayable: false,
        }
    }
}

pub struct Search {
    pub position: Position,
    pub global: Arc<Global>,
}

pub fn start() -> SyncSender<Search> {
    let (tx, rx) = sync_channel(0);
    spawn(move || {
        for Search { mut position, global } in rx {
            let f = position.frame_ptr();
            let mut node_limit = !0;
            let mut depth_limit = !0;
            for &limit in &global.limits {
                match limit {
                    Limit::Depth(depth) => depth_limit = depth_limit.min(depth),
                    Limit::Nodes(nodes) => node_limit = node_limit.min(nodes),
                    Limit::Ms(_) => {}
                }
            }
            let thread = &mut Thread::new(node_limit);
            let mut best = None;
            for depth in 1..=depth_limit {
                let result = catch_unwind(AssertUnwindSafe(|| search(&global, thread, f, depth, -i32::MAX, i32::MAX)));
                match result {
                    Ok((eval, mv)) => {
                        best = Sq::from_raw(mv);
                        let eval = Eval::Score(eval); // TODO: convert properly
                        let nodes = thread.nodes;
                        let time = global.elapsed();
                        let knps = if time == 0 { nodes } else { nodes / time };
                        send(&EngineMsg::Info { pv: vec![best.unwrap()], eval, depth, nodes, time, knps });
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
    tx
}
