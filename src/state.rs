use std::{arch::x86_64::_mm256_movemask_pd, mem::transmute, simd::prelude::*};

use multiptr::MultiMut;

pub struct Frame {
    pub opp_migo: u64,
    pub opp_yugo: u64,
    pub score: i32,
}

pub fn make(stack: MultiMut<Frame>, mv: u8) -> MakeResult {
    const DIRECTIONS: u64x4 = Simd::from_array([1, 9, 7, 8]);
    let [ref p, ref c, ref mut n] = *stack.as_array(-1);
    let bit = 1 << mv;
    let mut new_migo = p.opp_migo | bit;
    let mut new_yugo = p.opp_yugo;
    let mut new_score = c.score;
    let mut masks = Simd::splat(new_migo | new_yugo);
    masks &= masks >> DIRECTIONS;
    masks &= masks >> DIRECTIONS >> DIRECTIONS;
    masks &= Simd::from_array([0x1F1F1F1F1F1F1F1F, 0x0000001F1F1F1F1F, 0x000000FFFFFFFFFF, 0x000000F8F8F8F8F8]);
    let line_4 = masks;
    if line_4.reduce_or() == 0 {
        n.opp_migo = new_migo;
        n.opp_yugo = new_yugo;
        n.score = c.score;
        // todo check board fill? maybe only in search
        return MakeResult::Ok;
    }
    let line_5 = masks & masks >> DIRECTIONS;
    if line_5.reduce_or() != 0 {
        return MakeResult::Illegal;
    }
    masks |= masks << DIRECTIONS;
    masks |= masks << DIRECTIONS << DIRECTIONS;
    new_yugo |= bit;
    // optimize to vptest
    if Simd::splat(new_yugo).simd_eq(masks).to_int().reduce_or() != 0 {
        return MakeResult::GameOver(Outcome::Win);
    }
    new_migo &= !masks.reduce_or();
    new_score += unsafe { _mm256_movemask_pd(transmute(masks)) }.count_ones() as i32;
    n.opp_migo = new_migo;
    n.opp_yugo = new_yugo;
    n.score = c.score;
    // todo
}

pub enum MakeResult {
    Ok,
    Illegal,
    GameOver(Outcome),
}

pub enum Outcome {
    Win,
    Draw,
    Loss,
}
