use std::{
    array,
    panic::{AssertUnwindSafe, catch_unwind, panic_any},
    simd::prelude::*,
    sync::mpsc::{SyncSender, sync_channel},
    thread::spawn,
};

use parking_lot::{ArcRwLockReadGuard, RawRwLock};

use crate::{
    game::Game,
    protocol::{EngineMsg, Eval, mv::Mv},
    search::{ExitSearch, convert_eval, search},
    send, send_error,
    shared::Shared,
    thread::Thread,
};

pub enum Cmd {
    Reset,
    Sync,
    Go { shared: ArcRwLockReadGuard<RawRwLock, Shared> },
    Debug,
}

pub fn start() -> SyncSender<Cmd> {
    let (tx, rx) = sync_channel(0);
    spawn(move || {
        let mut histories = [i8x64::default(); 2];
        let mut game = Game::new(array::from_fn(|i| &raw mut histories[i]));
        for cmd in rx {
            match cmd {
                Cmd::Reset => {
                    for vec in &mut histories {
                        *vec = Simd::default();
                    }
                    game.searcher_reset();
                }
                Cmd::Sync => { /* rendezvous with main thread */ }
                Cmd::Go { shared } => {
                    game.sync_with(&shared.game);
                    let f = game.frame_ptr();
                    let thread = &mut Thread::new(shared.limits.nodes);
                    let mut best = None;
                    for depth in 1..=shared.limits.depth {
                        let result = catch_unwind(AssertUnwindSafe(|| {
                            search::<true>(&shared, thread, f, depth, -i32::MAX, i32::MAX)
                        }));
                        match result {
                            Ok(eval) => {
                                best = Mv::from_raw(f.pv[0]);
                                let pv = f.pv[..f.pv_len].iter().map(|&mv| Mv::from_raw(mv).unwrap()).collect();
                                let eval = convert_eval(f, eval);
                                let time_ns = shared.started_at.elapsed().as_nanos().max(1);
                                let time = (time_ns / 1_000_000) as u64;
                                let nodes = thread.nodes;
                                let evals = thread.evals;
                                let knps = (nodes as u128 * 1_000_000 / time_ns) as u64;
                                let keps = (evals as u128 * 1_000_000 / time_ns) as u64;
                                let pv_nodes = thread.pv_nodes;
                                send(&EngineMsg::Info { pv, eval, depth, time, nodes, knps, evals, keps, pv_nodes });
                                if matches!(eval, Eval::Decisive(_)) {
                                    break;
                                }
                            }
                            Err(e) => {
                                if e.downcast_ref::<ExitSearch>().is_none() {
                                    send_error("Search panicked - this is an engine bug");
                                    send(&EngineMsg::Best(None));
                                    panic_any(e);
                                }
                                break;
                            }
                        }
                    }
                    send(&EngineMsg::Best(best));
                    shared.set_active(false);
                }
                Cmd::Debug => {
                    // TODO: 2d, colorful, pretty print histories (heatmap style)
                    eprintln!("White histories {:?}", histories[0]);
                    eprintln!("Black histories {:?}", histories[1]);
                }
            }
        }
    });
    tx
}
