use std::{
    sync::atomic::{self, AtomicBool, AtomicU8},
    time::Instant,
};

use crate::{game::Game, protocol::limits::Limits, tt};

pub struct Shared {
    pub started_at: Instant,
    pub active: AtomicBool,
    pub limits: Limits,
    pub game: Game,
    pub tt: tt::Table,
    pub generation: AtomicU8,
}

impl Shared {
    pub fn new(tt_len: usize) -> Self {
        Self {
            started_at: Instant::now(),
            active: false.into(),
            limits: Default::default(),
            game: Game::default(),
            tt: tt::Table::new(tt_len),
            generation: 127u8.into(),
        }
    }

    pub fn active(&self) -> bool {
        self.active.load(atomic::Ordering::Relaxed)
    }

    pub fn set_active(&self, value: bool) {
        if self.active() != value {
            self.active.store(value, atomic::Ordering::Relaxed);
        }
    }

    pub fn go(&mut self, started_at: Instant, limits: Limits) {
        self.started_at = started_at;
        *self.active.get_mut() = true;
        self.limits = limits;
        *self.generation.get_mut() = self.generation.get_mut().wrapping_add(1);
    }

    pub fn reset(&mut self) {
        self.game.undo_all();
        self.tt.clear();
        *self.generation.get_mut() = 127;
    }
}
