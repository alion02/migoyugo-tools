use std::{cmp::Ordering, panic::resume_unwind, simd::prelude::*, sync::atomic};

use multiptr::MultiMut;
use myu_protocol::Limit;

use crate::state::{DIRECTIONS, Frame, Global, MakeResult, SHR_MASK, Thread, apply, make};

pub const MAX_VALUE: i32 = 0x7FFF;

pub struct ExitSearch;

pub fn search(
    global: &Global,
    thread: &mut Thread,
    f: MultiMut<Frame>,
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
    let [p, c] = f.as_array(-1);
    let mut open = !(p.opp_migo | p.opp_yugo | c.opp_migo | c.opp_yugo);
    let mut node_value = -i32::MAX;
    let mut node_mv = !0;
    depth -= 1;
    while open != 0 {
        let mv = open.trailing_zeros() as u8;
        match make(f, mv) {
            MakeResult::Ok(new) => {
                let value = -if depth == 0 {
                    let my_migos = c.opp_migo.count_ones() as i32;
                    let my_yugos = c.opp_yugo.count_ones() as i32;
                    let my_simd_pieces = Simd::splat(c.opp_migo | c.opp_yugo);
                    let my_coherence_masks = my_simd_pieces & my_simd_pieces >> DIRECTIONS & SHR_MASK[1];
                    let my_coherence = my_coherence_masks.count_ones().reduce_sum() as i32;

                    let opp_migos = new.migo.count_ones() as i32;
                    let opp_yugos = new.yugo.count_ones() as i32;
                    let opp_simd_pieces = Simd::splat(new.migo | new.yugo);
                    let opp_coherence_masks = opp_simd_pieces & opp_simd_pieces >> DIRECTIONS & SHR_MASK[1];
                    let opp_coherence = opp_coherence_masks.count_ones().reduce_sum() as i32;

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
                            break;
                        }
                    }
                }
            }
            MakeResult::Illegal => (),
            MakeResult::Igo => return (MAX_VALUE - (c.ply + 1), mv), // terminal state is technically the *next* ply
        }
        open &= open - 1;
    }
    if node_mv == !0 {
        // Wego: no legal moves
        node_value = match c.score.cmp(&0) {
            Ordering::Greater => MAX_VALUE - c.ply,
            Ordering::Equal => 0,
            Ordering::Less => c.ply - MAX_VALUE,
        };
    }
    (node_value, node_mv)
}
