use std::{
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    sync::{
        Arc,
        mpsc::{SyncSender, sync_channel},
    },
    thread::spawn,
};

use multiptr::MultiMut;
use myu_protocol::{EngineMsg, Eval, Limit, Sq, UserMsg};

use crate::{
    search::{ExitSearch, search},
    send, send_error,
    state::{Frame, Global, MakeResult, Thread, apply, gen_mv},
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
            stack: (-1..416)
                .map(|ply| Frame {
                    opp_migo: 0,
                    opp_yugo: 0,
                    opp_makes_yugo: 0,
                    opp_makes_igo: 0,
                    opp_too_long: 0,
                    score: 0,
                    psqt_value: 0,
                    ply,
                    killers: [0, 1],
                })
                .collect(),
            index: 1,
            unplayable: false,
        }
    }
}

pub enum Cmd {
    Msg(UserMsg),
    Start(Arc<Global>),
}

pub fn start() -> SyncSender<Cmd> {
    let (tx, rx) = sync_channel(0);
    spawn(move || {
        let mut position = Position::default();
        for cmd in rx {
            match cmd {
                Cmd::Msg(msg) => match msg {
                    UserMsg::Reset => position = Position::default(),
                    UserMsg::Sync => { /* rendezvous with main thread */ }
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
                        let original = position.index;
                        for mv in mvs {
                            if position.unplayable {
                                send_error("Cannot play on this game state");
                                break 'b;
                            }
                            let f = position.frame_ptr();
                            match gen_mv(f).make(f, mv.raw()) {
                                MakeResult::Ok(data) => apply(f.offset(1), data, true),
                                MakeResult::Illegal => {
                                    send_error("Sequence contains illegal move(s), cancelling");
                                    position.index = original;
                                    break 'b;
                                }
                                MakeResult::Igo => position.unplayable = true,
                            }
                            position.index += 1;
                        }
                    }
                    UserMsg::Go(_) | UserMsg::Stop => unreachable!(),
                },
                Cmd::Start(global) => 'b: {
                    if position.unplayable {
                        send_error("Cannot search this game state");
                        send(&EngineMsg::Best(None));
                        break 'b;
                    }
                    let f = position.frame_ptr();
                    let mut node_limit = !0;
                    let mut depth_limit = 64;
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
                        let result =
                            catch_unwind(AssertUnwindSafe(|| search(&global, thread, f, depth, -i32::MAX, i32::MAX)));
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
                                    send_error("Search panicked - this is a bug");
                                    resume_unwind(e);
                                }
                                break;
                            }
                        }
                    }
                    send(&EngineMsg::Best(best));
                }
            }
        }
    });
    tx
}
