use std::{
    cmp::Ordering, marker::PhantomData, panic::resume_unwind, simd::prelude::*, sync::atomic::Ordering::Relaxed,
};

use multiptr::MultiMut;

use crate::{
    game::{Frame, MAX_LEN},
    protocol::Eval,
    shared::Shared,
    state::{apply, make_migo, make_yugo, occ},
    thread::Thread,
    util::goto,
};

pub const MAX_VALUE: i32 = 0x7FFF;
pub const DECISIVE: i32 = MAX_VALUE - MAX_LEN as i32;

pub fn convert_eval(f: MultiMut<Frame>, eval: i32) -> Eval {
    if eval.abs() < DECISIVE {
        Eval::Score(eval)
    } else {
        let distance = (MAX_VALUE - eval.abs()) - f.ply;
        Eval::Decisive(eval.signum() * distance)
    }
}

pub struct ExitSearch;

pub trait Get<T> {
    fn get(self) -> Option<T>;
}

impl<T> Get<T> for T {
    fn get(self) -> Option<T> {
        Some(self)
    }
}

pub struct Empty<T>(PhantomData<T>);

pub fn empty<T>() -> Empty<T> {
    Empty(PhantomData)
}

impl<T> Get<T> for Empty<T> {
    fn get(self) -> Option<T> {
        None
    }
}

pub trait Node {
    const PV: bool;
    const ZW: bool = !Self::PV;
    type IfPv<T>: Get<T>;
}

pub struct Pv;

impl Node for Pv {
    const PV: bool = true;
    type IfPv<T> = T;
}

pub struct Zw;

impl Node for Zw {
    const PV: bool = false;
    type IfPv<T> = Empty<T>;
}

pub fn search<N: Node>(
    shared: &Shared,
    thread: &mut Thread,
    mut f: MultiMut<Frame>,
    mut depth: u32,
    alpha: N::IfPv<i32>,
    beta: i32,
) -> i32 {
    let mut alpha = alpha.get().unwrap_or(unsafe { beta.unchecked_sub(1) });
    if thread.tick_countdown() {
        if !shared.active()
            || thread.nodes >= shared.limits.nodes
            || shared.started_at.elapsed().as_millis() >= shared.limits.time as u128
        {
            resume_unwind(Box::new(ExitSearch));
        }
        thread.reset_countdown();
    }
    thread.nodes += 1;
    if N::PV {
        thread.pv_nodes += 1;
    }
    let mut playable = !occ(f) & !f[-1].opp_too_long;
    let makes_yugo = f[-1].opp_makes_yugo;
    let makes_igo = f[-1].opp_makes_igo & playable;
    if makes_igo != 0 {
        f.pv[0] = makes_igo.trailing_zeros() as u8;
        f.pv_len = 1;
        return MAX_VALUE - (f.ply + 1); // terminal state is technically the *next* ply
    }
    if playable == 0 {
        // Wego: no legal moves
        let best_value = match f.score.cmp(&0) {
            Ordering::Greater => MAX_VALUE - f.ply,
            Ordering::Equal => 0,
            Ordering::Less => f.ply - MAX_VALUE,
        };
        f.pv_len = 0;
        return best_value;
    }
    let tt_entry = N::PV.then(|| &shared.tt[f.hash]);
    let tt_sig = N::PV.then(|| tt_entry.unwrap().sig.load(Relaxed));
    let curr_sig = N::PV.then(|| f.hash as u8);
    let mut best_value = -i32::MAX;
    let mut best_mv = !0;
    depth -= 1;
    macro_rules! try_mv {
        ($mv:expr, $on_cut:expr) => {
            let mv = $mv;
            let new = if makes_yugo & 1 << mv != 0 { make_yugo(f, mv) } else { make_migo(f, mv) }; // helper loses perf
            let mut value;
            if depth == 0 {
                thread.evals += 1;
                value = new.score * 20 + new.psqt_value;
            } else {
                let f = f + 1;
                apply(f, new, depth >= 2);
                'recursion: {
                    if N::ZW || best_value != -i32::MAX {
                        value = -search::<Zw>(shared, thread, f, depth, empty(), -alpha);
                        if N::ZW || value <= alpha {
                            break 'recursion;
                        }
                    }
                    value = -search::<Pv>(shared, thread, f, depth, -beta, -alpha);
                }
            };
            if value > best_value {
                best_value = value;
                if value > alpha {
                    best_mv = mv;
                    if N::PV {
                        if depth == 0 {
                            f.pv[0] = best_mv;
                            f.pv_len = 1;
                        } else {
                            let [ref mut f, ref n] = *f.as_mut_array(0);
                            let len = n.pv_len;
                            f.pv[0] = best_mv;
                            f.pv[1..][..len].copy_from_slice(&n.pv[..len]);
                            f.pv_len = len + 1;
                        }
                    }
                    if value >= beta {
                        $on_cut
                    }
                    alpha = value;
                }
            }
        };
    }
    goto!(
        {
            if N::PV
                && tt_sig == curr_sig
                && let tt_mv = tt_entry.unwrap().mv.load(Relaxed)
                && playable & 1 << tt_mv != 0
            {
                try_mv!(tt_mv, break 'cut);
                playable &= !(1 << tt_mv);
            }
            let killer_0 = 1 << f.killers[0];
            if playable & killer_0 != 0 {
                try_mv!(f.killers[0], break 'killer_cut);
            }
            let killer_1 = 1 << f.killers[1];
            if playable & killer_1 != 0 {
                try_mv!(f.killers[1], break 'alt_killer_cut);
            }
            playable &= !killer_0;
            playable &= !killer_1;
            let mut mvs = makes_yugo & playable;
            while mvs != 0 {
                try_mv!(mvs.trailing_zeros() as u8, break 'cut);
                mvs &= mvs - 1;
            }
            playable &= !makes_yugo;
            let mut mvs = playable;
            if depth == 0 {
                while mvs != 0 {
                    try_mv!(mvs.trailing_zeros() as u8, break 'cut);
                    mvs &= mvs - 1;
                }
                break 'determine_node_type;
            }
            let mut failed = 0u64;
            let scores = unsafe { *f.history };
            while mvs != 0 {
                let scores = Mask::from_bitmask(mvs).select(scores, Simd::splat(i8::MIN));
                let mv = Simd::splat(scores.reduce_max()).simd_eq(scores).to_bitmask().trailing_zeros() as u8;
                try_mv!(mv, {
                    let history = unsafe { &mut *f.history };
                    const HIST_BITS: u32 = 6;
                    let bonus = (depth * depth).min(1 << HIST_BITS) as i32;
                    fn update(entry: &mut i8, direction: i32, change: i32) {
                        *entry += (direction * change - ((*entry as i32 * change) >> HIST_BITS)) as i8;
                    }
                    update(&mut history[mv as usize], 1, bonus);
                    while failed != 0 {
                        let mv = failed.trailing_zeros() as u8;
                        update(&mut history[mv as usize], -1, bonus / 2);
                        failed &= failed - 1;
                    }
                    break 'cut;
                });
                failed |= 1 << mv;
                mvs &= !(1 << mv);
            }
            break 'determine_node_type;
        },
        'determine_node_type: {
            if best_mv != !0 {
                break 'pv;
            } else {
                break 'all;
            }
        },
        'killer_cut: {
            break 'update_tt;
        },
        'alt_killer_cut: {
            f.killers = [best_mv, f.killers[0]];
            break 'update_tt;
        },
        'cut: {
            if f.killers[0] != best_mv {
                if f.killers[1] != best_mv {
                    f.killers[1] = best_mv;
                } else {
                    f.killers = [best_mv, f.killers[0]];
                }
            }
            break 'update_tt;
        },
        'pv: {
            break 'update_tt;
        },
        'all: {
            break 'end;
        },
        'update_tt: {
            if N::PV {
                tt_entry.unwrap().mv.store(best_mv, Relaxed);
                tt_entry.unwrap().sig.store(curr_sig.unwrap(), Relaxed);
            }
            break 'end;
        },
        'end: {},
    );
    best_value
}
