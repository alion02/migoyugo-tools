use std::{
    arch::x86_64::_mm256_movemask_pd, mem::transmute, simd::prelude::*, sync::atomic::AtomicBool, time::Instant,
};

use multiptr::MultiMut;

use crate::protocol::Limits;

pub struct Global {
    pub started_at: Instant,
    pub stop: AtomicBool,
    pub node: Limits,
    pub ms: Limits,
}

pub struct Thread {
    pub nodes: u64,
    countdown: u32,
}

impl Thread {
    pub fn tick_countdown(&mut self) -> bool {
        self.countdown -= 1;
        self.countdown == 0
    }

    pub fn reset_countdown(&mut self, max: u32) {
        self.countdown = max.min(8192);
    }
}

pub struct Frame {
    pub opp_migo: u64,
    pub opp_yugo: u64,
    pub score: i32,
    pub ply: i32,
}

pub fn make(f: MultiMut<Frame>, mv: u8) -> MakeResult {
    const DIRECTIONS: u64x4 = Simd::from_array([1, 9, 7, 8]);
    let [p, c] = f.as_array(-1);
    let bit = 1 << mv;
    let mut migo = p.opp_migo | bit;
    let mut yugo = p.opp_yugo;
    let mut score = c.score;
    let mut masks = Simd::splat(migo | yugo);
    masks &= masks >> DIRECTIONS;
    masks &= masks >> DIRECTIONS >> DIRECTIONS;
    masks &= Simd::from_array([0x1F1F1F1F1F1F1F1F, 0x0000001F1F1F1F1F, 0x000000FFFFFFFFFF, 0x000000F8F8F8F8F8]);
    'b: {
        if masks.reduce_or() == 0 {
            // no 4 line
            break 'b;
        }
        let line_5 = masks & masks >> DIRECTIONS;
        if line_5.reduce_or() != 0 {
            // has 5+ line
            return MakeResult::Illegal;
        }
        // no 5+ and at least one 4 line
        masks |= masks << DIRECTIONS;
        masks |= masks << DIRECTIONS << DIRECTIONS;
        yugo |= bit;
        if (!Simd::splat(yugo) & masks).simd_ne(Simd::default()).any() {
            // at least one 4 line of yugos
            return MakeResult::Igo;
        }
        migo &= !masks.reduce_or();
        score += unsafe { _mm256_movemask_pd(transmute(masks)) }.count_ones() as i32;
    }
    score *= -1;
    MakeResult::Ok { migo, yugo, score }
}

pub enum MakeResult {
    Ok { migo: u64, yugo: u64, score: i32 },
    Illegal,
    Igo,
}
