use std::{ptr::null_mut, simd::prelude::*};

use multiptr::MultiMut;

use crate::{
    protocol::{limits::MAX_DEPTH, mv::Mv},
    state::{DirectMakeResult, checked_direct_make},
};

const LOOKBEHIND: usize = 1;
const LOOKAHEAD: usize = 0;
pub const MAX_LEN: usize = 64 * (4 * 3 + 1); // upper bound assuming board fill with inefficient quad-yugos

#[derive(Debug, Clone)]
pub struct Game {
    pub stack: Box<[Frame]>,
    pub index: usize,
}

impl Default for Game {
    fn default() -> Self {
        Self::new([null_mut(); 2])
    }
}

impl Game {
    pub fn new(histories: [*mut i8x64; 2]) -> Self {
        Self {
            stack: (-(LOOKBEHIND as i32)..(MAX_LEN + LOOKAHEAD) as i32)
                .map(|ply| Frame {
                    opp_migo: 0,
                    opp_yugo: 0,
                    opp_makes_yugo: 0,
                    opp_makes_igo: 0,
                    opp_too_long: 0,
                    score: 0,
                    psqt_value: 0,
                    hash: 0,

                    ply,
                    killers: [0, 1],
                    history: histories[ply as usize & 1],
                    pv: [0; _],
                    pv_len: 0,
                })
                .collect(),
            index: LOOKBEHIND,
        }
    }

    pub fn frame_ptr(&mut self) -> MultiMut<'_, Frame> {
        unsafe { MultiMut::from_slice_index(&mut self.stack, self.index) }
    }

    pub fn play(&mut self, mvs: &[Mv], from_start: bool) -> Result<(), &'static str> {
        let index = self.index;
        let err = 'update: {
            if from_start {
                self.undo_all();
            }
            for mv in mvs {
                match checked_direct_make(self.frame_ptr(), mv.raw()) {
                    DirectMakeResult::Ok => (),
                    DirectMakeResult::Igo => break 'update "Move sequence extends past Igo, cancelling",
                    DirectMakeResult::Wego => break 'update "Move sequence extends past Wego, cancelling",
                    DirectMakeResult::Illegal => break 'update "Sequence contains illegal move(s), cancelling",
                }
                self.index += 1;
            }
            return Ok(());
        };
        self.index = index;
        Err(err)
    }

    pub fn undo(&mut self, count: usize) -> Result<(), &'static str> {
        if count > self.index - LOOKBEHIND {
            return Err("Requested too many moves to undo, cancelling");
        }
        self.index -= count;
        Ok(())
    }

    pub fn undo_all(&mut self) {
        self.index = LOOKBEHIND;
    }

    pub fn searcher_reset(&mut self) {
        for frame in &mut self.stack {
            frame.killers = [0, 1];
        }
    }

    pub fn sync_with(&mut self, game: &Game) {
        self.index = game.index;
        for i in self.index - LOOKBEHIND..self.index + LOOKAHEAD + 1 {
            let this = &mut self.stack[i];
            let other = game.stack[i];
            this.opp_migo = other.opp_migo;
            this.opp_yugo = other.opp_yugo;
            this.opp_makes_yugo = other.opp_makes_yugo;
            this.opp_makes_igo = other.opp_makes_igo;
            this.opp_too_long = other.opp_too_long;
            this.score = other.score;
            this.psqt_value = other.psqt_value;
            this.hash = other.hash;
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Frame {
    pub opp_migo: u64,
    pub opp_yugo: u64,
    pub opp_makes_yugo: u64,
    pub opp_makes_igo: u64,
    pub opp_too_long: u64,
    pub score: i32,
    pub psqt_value: i32,
    pub hash: u64,

    pub ply: i32,
    pub killers: [u8; 2],
    pub history: *mut i8x64,
    pub pv: [u8; MAX_DEPTH as usize],
    pub pv_len: usize,
}

unsafe impl Send for Frame {}
unsafe impl Sync for Frame {}
