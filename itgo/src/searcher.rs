use std::{
    array,
    panic::{AssertUnwindSafe, catch_unwind, panic_any},
    simd::prelude::*,
    sync::{
        Arc, RwLock,
        mpsc::{SyncSender, sync_channel},
    },
    thread::spawn,
};

use myu_protocol::{EngineMsg, Eval, Sq};

use crate::{
    game::Game,
    search::{ExitSearch, search},
    send, send_error,
    shared::Shared,
    thread::Thread,
};

pub enum Cmd {
    Reset,
    Sync,
    Go,
    Debug,
}

pub fn start(shared: Arc<RwLock<Shared>>) -> SyncSender<Cmd> {
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
                }
                Cmd::Sync => { /* rendezvous with main thread */ }
                Cmd::Go => {
                    let shared = shared.read().unwrap();
                    game.sync_with(&shared.game);
                    let f = game.frame_ptr();
                    let thread = &mut Thread::new(shared.limits.nodes);
                    let mut best = None;
                    for depth in 1..=shared.limits.depth {
                        let result =
                            catch_unwind(AssertUnwindSafe(|| search(&shared, thread, f, depth, -i32::MAX, i32::MAX)));
                        match result {
                            Ok((eval, mv)) => {
                                best = Sq::from_raw(mv);
                                let eval = Eval::Score(eval); // TODO: convert properly
                                let time_ns = shared.started_at.elapsed().as_nanos().max(1);
                                let time = (time_ns / 1_000_000) as u64;
                                let nodes = thread.nodes;
                                let evals = thread.evals;
                                let knps = (nodes as u128 * 1_000_000 / time_ns) as u64;
                                let keps = (evals as u128 * 1_000_000 / time_ns) as u64;
                                send(&EngineMsg::Info {
                                    pv: vec![best.unwrap()],
                                    eval,
                                    depth,
                                    time,
                                    nodes,
                                    knps,
                                    evals,
                                    keps,
                                });
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
                    eprintln!("White histories: {:?}", histories[0]);
                    eprintln!("Black histories: {:?}", histories[1]);
                }
            }
        }
    });
    tx
}
