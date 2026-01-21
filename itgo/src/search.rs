use std::{cmp::Ordering, panic::resume_unwind, sync::atomic};

use multiptr::MultiMut;
use myu_protocol::Limit;

use crate::state::{Frame, GenMvData, Global, Thread, apply, gen_mv, make_migo, make_yugo};

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
    let makes_igo = f[-1].opp_makes_igo & playable;
    if makes_igo != 0 {
        return (MAX_VALUE - (f.ply + 1), makes_igo.trailing_zeros() as u8); // terminal state is technically the *next* ply
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
                new.score * 20 + new.psqt_value
            } else {
                let f = f + 1;
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
