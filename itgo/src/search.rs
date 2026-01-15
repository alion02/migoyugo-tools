use std::{cmp::Ordering, panic::resume_unwind, simd::prelude::*, sync::atomic};

use multiptr::MultiMut;
use myu_protocol::Limit;

use crate::state::{DIRECTIONS, Frame, Global, MakeResult, SHR_MASK, Thread, apply, make};

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
    let open = !(f[-1].opp_migo | f[-1].opp_yugo | f.opp_migo | f.opp_yugo);
    let killer_0 = 1 << f.killers[0];
    let killer_1 = 1 << f.killers[1];
    let mut node_value = -i32::MAX;
    let mut node_mv = !0;
    depth -= 1;
    'moves: for mut open in [open & killer_0, open & killer_1, open & !killer_0 & !killer_1] {
        while open != 0 {
            let mv = open.trailing_zeros() as u8;
            match make(f, mv) {
                MakeResult::Ok(new) => {
                    let value = -if depth == 0 {
                        let my_migos = f.opp_migo.count_ones() as i32;
                        let my_yugos = f.opp_yugo.count_ones() as i32;
                        let my_simd_pieces = Simd::splat(f.opp_migo | f.opp_yugo);
                        let my_coherence_masks_near = my_simd_pieces & my_simd_pieces >> DIRECTIONS & SHR_MASK[1];
                        let my_coherence_masks_far =
                            my_simd_pieces & my_simd_pieces >> DIRECTIONS >> DIRECTIONS & SHR_MASK[2];
                        let my_coherence = my_coherence_masks_near.count_ones().reduce_sum() as i32
                            + my_coherence_masks_far.count_ones().reduce_sum() as i32;

                        let opp_migos = new.migo.count_ones() as i32;
                        let opp_yugos = new.yugo.count_ones() as i32;
                        let opp_simd_pieces = Simd::splat(new.migo | new.yugo);
                        let opp_coherence_masks_near = opp_simd_pieces & opp_simd_pieces >> DIRECTIONS & SHR_MASK[1];
                        let opp_coherence_masks_far =
                            opp_simd_pieces & opp_simd_pieces >> DIRECTIONS >> DIRECTIONS & SHR_MASK[2];
                        let opp_coherence = opp_coherence_masks_near.count_ones().reduce_sum() as i32
                            + opp_coherence_masks_far.count_ones().reduce_sum() as i32;

                        new.score * 16
                            + (my_migos - opp_migos)
                            + (my_yugos - opp_yugos) * 64
                            + (my_coherence - opp_coherence)
                    } else {
                        let f = f.offset(1);
                        apply(f, new);
                        search(global, thread, f, depth, -beta, -alpha).0
                    };
                    if value > node_value {
                        node_value = value;
                        node_mv = mv;
                        if value > alpha {
                            alpha = value;
                            if alpha >= beta {
                                if f.killers[0] != mv {
                                    if f.killers[1] != mv {
                                        f.killers[1] = mv;
                                    } else {
                                        f.killers = [mv, f.killers[0]];
                                    }
                                }
                                break 'moves;
                            }
                        }
                    }
                }
                MakeResult::Illegal => (),
                MakeResult::Igo => return (MAX_VALUE - (f.ply + 1), mv), // terminal state is technically the *next* ply
            }
            open &= open - 1;
        }
    }
    if node_mv == !0 {
        // Wego: no legal moves
        node_value = match f.score.cmp(&0) {
            Ordering::Greater => MAX_VALUE - f.ply,
            Ordering::Equal => 0,
            Ordering::Less => f.ply - MAX_VALUE,
        };
    }
    (node_value, node_mv)
}
