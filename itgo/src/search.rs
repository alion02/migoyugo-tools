use std::{cmp::Ordering, panic::resume_unwind, simd::prelude::*, sync::atomic};

use multiptr::MultiMut;
use myu_protocol::Limit;

use crate::state::{DIRS, Frame, GenMvData, Global, SHR_MASK, Thread, apply, gen_mv, has_line, make_migo, make_yugo};

pub const MAX_VALUE: i32 = 0x7FFF;

pub struct ExitSearch;

pub fn search(
    global: &Global,
    thread: &mut Thread,
    mut f: MultiMut<Frame>,
    mut depth: u32,
    mut alpha: i32,
    beta: i32,
) -> (i32, u8) {
    if thread.tick_countdown() {
        if global.stop.load(atomic::Ordering::Relaxed)
            || global.limits.iter().any(|&limit| match limit {
                Limit::Depth(_) => false,
                Limit::Nodes(nodes) => thread.nodes >= nodes,
                Limit::Ms(ms) => global.elapsed() >= ms,
            })
        {
            resume_unwind(Box::new(ExitSearch));
        }
        thread.reset_countdown();
    }
    thread.nodes += 1;
    let GenMvData { playable, makes_yugo } = gen_mv(f);
    let mut mask = makes_yugo;
    while mask != 0 {
        let mv = mask.trailing_zeros() as u8;
        if has_line(f[-1].opp_yugo | 1 << mv) {
            return (MAX_VALUE - (f.ply + 1), mv); // terminal state is technically the *next* ply
        }
        mask &= mask - 1;
    }
    if playable == 0 {
        // Wego: no legal moves
        let best_value = match f.score.cmp(&0) {
            Ordering::Greater => MAX_VALUE - f.ply,
            Ordering::Equal => 0,
            Ordering::Less => f.ply - MAX_VALUE,
        };
        return (best_value, !0);
    }
    let killer_0 = 1 << f.killers[0];
    let killer_1 = 1 << f.killers[1];
    let mut best_value = -i32::MAX;
    let mut best_mv = !0;
    depth -= 1;
    'moves: for mut open in [
        playable & killer_0,
        playable & killer_1,
        makes_yugo & !killer_0 & !killer_1,
        playable & !makes_yugo & !killer_0 & !killer_1,
    ] {
        while open != 0 {
            let mv = open.trailing_zeros() as u8;
            let new = if makes_yugo & 1 << mv != 0 { make_yugo(f, mv) } else { make_migo(f, mv) }; // helper loses perf
            let value = if depth == 0 {
                struct EvalData {
                    coherence: i32,
                }

                #[inline]
                fn side_eval(migo: u64, yugo: u64) -> EvalData {
                    let pieces = migo | yugo;
                    let simd_pieces = Simd::splat(pieces);
                    let coherence_masks_near = simd_pieces & simd_pieces >> DIRS[1] & SHR_MASK[1];
                    let coherence_masks_far = simd_pieces & simd_pieces >> DIRS[2] & SHR_MASK[2];
                    let coherence = coherence_masks_near.count_ones().reduce_sum() as i32
                        + coherence_masks_far.count_ones().reduce_sum() as i32;
                    EvalData { coherence }
                }

                let my = side_eval(new.migo, new.yugo);
                let opp = side_eval(f.opp_migo, f.opp_yugo);
                new.score * 16 + new.psqt_value + (my.coherence - opp.coherence)
            } else {
                let f = f.offset(1);
                apply(f, new, depth >= 2);
                -search(global, thread, f, depth, -beta, -alpha).0
            };
            if value > best_value {
                best_value = value;
                best_mv = mv;
                if value > alpha {
                    if value >= beta {
                        break 'moves;
                    }
                    alpha = value;
                }
            }
            open &= open - 1;
        }
    }
    if best_value >= beta {
        if f.killers[0] != best_mv {
            if f.killers[1] != best_mv {
                f.killers[1] = best_mv;
            } else {
                f.killers = [best_mv, f.killers[0]];
            }
        }
    }
    (best_value, best_mv)
}
