use std::{
    cmp::Ordering,
    sync::{
        Arc,
        atomic::{self, AtomicBool},
    },
};

use multiptr::MultiMut;

use crate::state::{Frame, MakeResult, make};

pub const MAX_VALUE: i32 = 0x1000;

pub struct Thread {
    stop: Arc<AtomicBool>,
    check: u32,
}

pub fn search(thread: &mut Thread, mut f: MultiMut<Frame>, mut depth: u32) -> (i32, u8) {
    thread.check -= 1;
    if thread.check == 0 {
        if thread.stop.load(atomic::Ordering::Relaxed) {
            panic!();
        }
        thread.check = 2048;
    }
    let [p, c] = f.as_array(-1);
    let mut open = !(p.opp_migo | p.opp_yugo | c.opp_migo | c.opp_yugo);
    let mut node_value = -i32::MAX;
    let mut node_mv = !0;
    depth -= 1;
    while open != 0 {
        let mv = open.trailing_zeros() as u8;
        match make(f, mv) {
            MakeResult::Ok { migo, yugo, score } => {
                f = f.offset(1);
                let value = -if depth == 0 {
                    score
                } else {
                    f.opp_migo = migo;
                    f.opp_yugo = yugo;
                    f.score = score;
                    search(thread, f, depth).0
                };
                if value > node_value {
                    node_value = value;
                    node_mv = mv;
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
