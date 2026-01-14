use std::{
    arch::x86_64::_mm256_movemask_pd, mem::transmute, num::Wrapping, simd::prelude::*, sync::atomic::AtomicBool,
    time::Instant,
};

use multiptr::MultiMut;

use crate::protocol::Limit;

pub struct Global {
    pub started_at: Instant,
    pub stop: AtomicBool,
    pub limits: Vec<Limit>,
}

impl Global {
    pub fn elapsed(&self) -> u64 {
        self.started_at.elapsed().as_millis() as u64
    }
}

pub struct Thread {
    pub nodes: u64,
    pub node_limit: u64,
    countdown: Wrapping<u32>,
}

impl Thread {
    pub fn new(node_limit: u64) -> Self {
        Self { nodes: 0, node_limit, countdown: Wrapping(0) }
    }

    pub fn tick_countdown(&mut self) -> bool {
        self.countdown -= 1;
        self.countdown.0 == !0
    }

    pub fn reset_countdown(&mut self) {
        self.countdown.0 = (self.node_limit - self.nodes).min(8192) as u32;
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Frame {
    pub opp_migo: u64,
    pub opp_yugo: u64,
    pub score: i32,
    pub ply: i32,
}

pub enum MakeResult {
    Ok(MakeData),
    Illegal,
    Igo,
}

pub struct MakeData {
    pub migo: u64,
    pub yugo: u64,
    pub score: i32,
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
    MakeResult::Ok(MakeData { migo, yugo, score })
}

pub fn apply(mut f: MultiMut<Frame>, make: MakeData) {
    f.opp_migo = make.migo;
    f.opp_yugo = make.yugo;
    f.score = make.score;
}
