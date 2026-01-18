use std::{
    arch::x86_64::_mm256_movemask_pd, mem::transmute, num::Wrapping, simd::prelude::*, sync::atomic::AtomicBool,
    time::Instant,
};

use multiptr::MultiMut;
use myu_protocol::Limit;

pub static DIRS: [u64x4; 8] = {
    let mut simd_dirs = [[0u64; 4]; 8];
    let mut i = 0;
    while i < 8 {
        simd_dirs[i as usize] = [i, i * 9, i * 8, i * 7];
        i += 1;
    }
    unsafe { transmute(simd_dirs) }
};
pub static SHR_MASK: [u64x4; 8] = {
    let mut simd_masks = [[!0u64; 4]; 8];
    simd_masks[1] = [0x7F7F7F7F7F7F7F7F, 0x007F7F7F7F7F7F7F, 0x00FFFFFFFFFFFFFF, 0x00FEFEFEFEFEFEFE];
    let mut i = 2;
    while i < 8 {
        let mut j = 0;
        while j < 4 {
            simd_masks[i][j] = simd_masks[i - 1][j] & simd_masks[i - 1][j] >> DIRS[1].as_array()[j];
            j += 1;
        }
        i += 1;
    }
    unsafe { transmute(simd_masks) }
};
pub static SHL_MASK: [u64x4; 8] = {
    let mut simd_masks = [[!0u64; 4]; 8];
    simd_masks[1] = [0xFEFEFEFEFEFEFEFE, 0xFEFEFEFEFEFEFE00, 0xFFFFFFFFFFFFFF00, 0x7F7F7F7F7F7F7F00];
    let mut i = 2;
    while i < 8 {
        let mut j = 0;
        while j < 4 {
            simd_masks[i][j] = simd_masks[i - 1][j] & simd_masks[i - 1][j] << DIRS[1].as_array()[j];
            j += 1;
        }
        i += 1;
    }
    unsafe { transmute(simd_masks) }
};

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
        Self { nodes: 0, node_limit, countdown: Wrapping(1) }
    }

    pub fn tick_countdown(&mut self) -> bool {
        self.countdown -= 1;
        self.countdown.0 == 0
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
    pub killers: [u8; 2],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MakeResult {
    Ok(MakeData),
    Igo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MakeData {
    pub migo: u64,
    pub yugo: u64,
    pub score: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenMvResult {
    Ok(GenMvData),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GenMvData {
    pub playable: u64,
    pub makes_yugo: u64,
}

pub fn make_migo(f: MultiMut<Frame>, mv: u8) -> MakeData {
    MakeData { migo: f[-1].opp_migo | 1 << mv, yugo: f[-1].opp_yugo, score: f.score }
}

pub fn make(f: MultiMut<Frame>, mv: u8) -> MakeResult {
    let [p, c] = f.as_array(-1);
    let bit = 1 << mv;
    let mut migo = p.opp_migo | bit;
    let mut yugo = p.opp_yugo;
    let mut score = c.score;
    let mut masks = Simd::splat(migo | yugo);
    masks &= masks >> DIRS[1];
    masks &= masks >> DIRS[2];
    masks &= SHR_MASK[3];
    let line_4 = masks;
    'b: {
        if line_4.reduce_or() == 0 {
            // no 4 line
            break 'b;
        }
        // at least one 4 line
        masks |= masks << DIRS[1];
        masks |= masks << DIRS[2];
        yugo |= bit;
        if ((Simd::splat(yugo) & masks).simd_eq(masks).to_int() & masks.cast()).reduce_or() != 0 {
            // at least one 4 line of yugos
            return MakeResult::Igo;
        }
        migo &= !masks.reduce_or();
        score += unsafe { _mm256_movemask_pd(transmute(line_4.simd_ne(Simd::default()))) }.count_ones() as i32;
    }
    MakeResult::Ok(MakeData { migo, yugo, score })
}

pub fn apply(mut f: MultiMut<Frame>, make: MakeData) {
    f.opp_migo = make.migo;
    f.opp_yugo = make.yugo;
    f.score = -make.score;
}

pub fn gen_mv(f: MultiMut<Frame>) -> GenMvResult {
    let own = Simd::splat(own(f));
    let line_2 = own & own >> DIRS[1];
    let line_3 = line_2 & own << DIRS[1];
    let ext_three_a = line_3 >> DIRS[2] & SHR_MASK[3];
    let ext_three_b = line_3 << DIRS[2] & SHL_MASK[3];
    let two_one_a = own << DIRS[1] & line_2 >> DIRS[1] & SHR_MASK[2] & SHL_MASK[1];
    let two_one_b = own >> DIRS[1] & line_2 << DIRS[2] & SHR_MASK[1] & SHL_MASK[2];
    let pi_a = ext_three_a & two_one_a;
    let pi_b = ext_three_b & two_one_b;
    let two_two = two_one_a & two_one_b;
    let too_long = pi_a | pi_b | two_two;
    let too_long = too_long.reduce_or();
    let playable = !occ(f) & !too_long;
    let makes_yugo = ext_three_a | ext_three_b | two_one_a | two_one_b;
    let makes_yugo = makes_yugo.reduce_or() & playable;
    GenMvResult::Ok(GenMvData { playable, makes_yugo })
}

pub fn opp(f: MultiMut<Frame>) -> u64 {
    f.opp_migo | f.opp_yugo
}

pub fn own(f: MultiMut<Frame>) -> u64 {
    opp(f.offset(-1))
}

pub fn occ(f: MultiMut<Frame>) -> u64 {
    own(f) | opp(f)
}
