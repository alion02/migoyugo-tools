use std::{ptr::null_mut, simd::prelude::*};

use multiptr::MultiMut;
use myu_protocol::Sq;

const LOOKBEHIND: usize = 1;
const LOOKAHEAD: usize = 0;
const MAX_LEN: usize = 64 * (4 * 3 + 1); // upper bound assuming board fill with inefficient quad-yugos

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
                    ply,
                    killers: [0, 1],
                    history: histories[ply as usize & 1],
                })
                .collect(),
            index: LOOKBEHIND,
        }
    }

    pub fn frame_ptr(&mut self) -> MultiMut<'_, Frame> {
        unsafe { MultiMut::from_slice_index(&mut self.stack, self.index) }
    }

    pub fn play(&mut self, mvs: &[Sq]) -> Result<(), &'static str> {
        let index = self.index;
        let err = 'update: {
            for mv in mvs {
                todo!(); // checked_apply
                // if self.is_over() {
                //     break 'update "Move sequence extends past the end of the game, cancelling";
                // }
                // match gen_mv(f).make(f, mv.raw()) {
                //     MakeResult::Ok(data) => apply(f + 1, data, true),
                //     MakeResult::Illegal => break 'update "Sequence contains illegal move(s), cancelling",
                //     MakeResult::Igo => self.unplayable = true,
                // }
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

    pub fn reset(&mut self) {
        self.index = LOOKBEHIND;
    }

    pub fn sync_with(&mut self, game: &Game) {
        self.index = game.index;
        for i in self.index - LOOKBEHIND..self.index + LOOKAHEAD + 1 {
            let this = &mut self.stack[i];
            let other = game.stack[i];
            *this = Frame {
                opp_migo: other.opp_migo,
                opp_yugo: other.opp_yugo,
                opp_makes_yugo: other.opp_makes_yugo,
                opp_makes_igo: other.opp_makes_igo,
                opp_too_long: other.opp_too_long,
                score: other.score,
                psqt_value: other.psqt_value,
                ply: this.ply,
                killers: this.killers,
                history: this.history,
            }
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
    pub ply: i32,
    pub killers: [u8; 2],
    pub history: *mut i8x64,
}

unsafe impl Send for Frame {}
unsafe impl Sync for Frame {}
