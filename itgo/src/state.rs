use std::{arch::x86_64::_mm256_movemask_pd, mem::transmute, simd::prelude::*};

use multiptr::MultiMut;

use crate::{game::Frame, tt::HASH_STM, util::assume};

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

const fn to_symmetrical<T: Copy>(v: [T; 10]) -> [T; 64] {
    let mut out = [[v[0]; 8]; 8];
    let mut i = 0;
    let mut s = 0;
    while s < 4 {
        let mut z = s;
        while z < 4 {
            let val = v[i];
            out[s][z] = val;
            out[s][7 - z] = val;
            out[7 - s][z] = val;
            out[7 - s][7 - z] = val;
            out[z][s] = val;
            out[z][7 - s] = val;
            out[7 - z][s] = val;
            out[7 - z][7 - s] = val;
            z += 1;
            i += 1;
        }
        s += 1;
    }
    unsafe { (&raw const out).cast::<[T; 64]>().read() }
}

pub static PSQT_MIGO: [i32; 64] = to_symmetrical([2, 3, 4, 6, 6, 7, 9, 12, 11, 5]);

pub static PSQT_YUGO: [i32; 64] = to_symmetrical([50, 52, 54, 56, 59, 61, 61, 65, 65, 70]);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MakeData {
    pub migo: u64,
    pub yugo: u64,
    pub score: i32,
    pub psqt_value: i32,
    pub hash: u64,
}

pub fn make_migo(f: MultiMut<Frame>, mv: u8) -> MakeData {
    assume!((mv as usize) < 64);
    MakeData {
        migo: f[-1].opp_migo | 1 << mv,
        yugo: f[-1].opp_yugo,
        score: f.score,
        psqt_value: f.psqt_value + PSQT_MIGO[mv as usize],
        hash: f.hash ^ f.side_hash.migo()[mv as usize],
    }
}

pub fn make_yugo(f: MultiMut<Frame>, mv: u8) -> MakeData {
    assume!((mv as usize) < 64);
    let mut migo = f[-1].opp_migo;
    let yugo = f[-1].opp_yugo | 1 << mv;
    let mut score = f.score;
    let mut psqt_value = f.psqt_value + PSQT_YUGO[mv as usize];
    let mut hash = f.hash ^ f.side_hash.yugo()[mv as usize];
    let mut masks = Simd::splat(migo | yugo);
    masks &= masks >> DIRS[1];
    masks &= masks >> DIRS[2];
    masks &= SHR_MASK[3];
    let has_line = masks;
    masks |= masks << DIRS[1];
    masks |= masks << DIRS[2];
    let lines = masks.reduce_or();
    let mut remove = migo & lines;
    while remove != 0 {
        let idx = remove.trailing_zeros() as usize;
        psqt_value -= PSQT_MIGO[idx];
        hash ^= f.side_hash.migo()[idx];
        remove &= remove - 1;
    }
    migo &= !lines;
    score += unsafe { _mm256_movemask_pd(transmute(has_line.simd_ne(Simd::default()))) }.count_ones() as i32;
    MakeData { migo, yugo, score, psqt_value, hash }
}

pub fn apply(mut f: MultiMut<Frame>, make: MakeData, compute_aux: bool) {
    if compute_aux {
        let AlmostLines { ext_three_a, ext_three_b, two_one_a, two_one_b, any_line: makes_yugo } =
            almost_lines(make.migo | make.yugo);
        let makes_yugo = makes_yugo.reduce_or();
        let pi_a = ext_three_a & two_one_a;
        let pi_b = ext_three_b & two_one_b;
        let two_two = two_one_a & two_one_b;
        let too_long = pi_a | pi_b | two_two;
        let too_long = too_long.reduce_or();
        f.opp_makes_yugo = makes_yugo;
        f.opp_makes_igo = almost_lines(make.yugo).any_line.reduce_or();
        f.opp_too_long = too_long;
    }
    f.opp_migo = make.migo;
    f.opp_yugo = make.yugo;
    f.score = -make.score;
    f.psqt_value = -make.psqt_value;
    f.hash = make.hash ^ HASH_STM;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GenMvData {
    pub playable: u64,
    pub makes_yugo: u64,
}

pub fn gen_mv(f: MultiMut<Frame>) -> GenMvData {
    let playable = !occ(f) & !f[-1].opp_too_long;
    let makes_yugo = f[-1].opp_makes_yugo & playable;
    GenMvData { playable, makes_yugo }
}

pub enum DirectMakeResult {
    Ok,
    Igo,
    Wego,
    Illegal,
}

pub fn checked_direct_make(f: MultiMut<Frame>, mv: u8) -> DirectMakeResult {
    if has_line(f.opp_yugo) {
        return DirectMakeResult::Igo;
    }
    let GenMvData { playable, makes_yugo } = gen_mv(f);
    if playable == 0 {
        return DirectMakeResult::Wego;
    }
    if playable & 1 << mv == 0 {
        return DirectMakeResult::Illegal;
    }
    let make = if makes_yugo & 1 << mv == 0 { make_migo(f, mv) } else { make_yugo(f, mv) };
    apply(f + 1, make, true);
    DirectMakeResult::Ok
}

pub struct AlmostLines {
    pub ext_three_a: u64x4,
    pub ext_three_b: u64x4,
    pub two_one_a: u64x4,
    pub two_one_b: u64x4,
    pub any_line: u64x4,
}

pub fn almost_lines(mask: u64) -> AlmostLines {
    let masks = Simd::splat(mask);
    let line_2 = masks & masks >> DIRS[1];
    let line_3 = line_2 & masks << DIRS[1];
    let ext_three_a = line_3 >> DIRS[2] & SHR_MASK[3];
    let ext_three_b = line_3 << DIRS[2] & SHL_MASK[3];
    let two_one_a = masks << DIRS[1] & line_2 >> DIRS[1] & SHR_MASK[2] & SHL_MASK[1];
    let two_one_b = masks >> DIRS[1] & line_2 << DIRS[2] & SHR_MASK[1] & SHL_MASK[2];
    let makes_line = ext_three_a | ext_three_b | two_one_a | two_one_b;
    AlmostLines { ext_three_a, ext_three_b, two_one_a, two_one_b, any_line: makes_line }
}

pub fn has_line(mask: u64) -> bool {
    let mut masks = Simd::splat(mask);
    masks &= masks >> DIRS[1];
    masks &= masks >> DIRS[2];
    masks &= SHR_MASK[3];
    masks.reduce_or() != 0
}

pub fn opp(f: MultiMut<Frame>) -> u64 {
    f.opp_migo | f.opp_yugo
}

pub fn own(f: MultiMut<Frame>) -> u64 {
    opp(f - 1)
}

pub fn occ(f: MultiMut<Frame>) -> u64 {
    own(f) | opp(f)
}
