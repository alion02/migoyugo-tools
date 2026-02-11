use std::{
    sync::atomic::{self, AtomicBool},
    time::Instant,
};

use crate::{game::Game, protocol::limits::Limits, tt};

pub struct Shared {
    pub started_at: Instant,
    pub active: AtomicBool,
    pub limits: Limits,
    pub game: Game,
    pub tt: tt::Table,
}

impl Shared {
    pub fn new(tt_len: usize) -> Self {
        Self {
            started_at: Instant::now(),
            active: false.into(),
            limits: Default::default(),
            game: Game::default(),
            tt: tt::Table::new(tt_len),
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
    }
}
