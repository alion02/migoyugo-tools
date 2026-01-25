use std::{
    sync::atomic::{self, AtomicBool},
    time::Instant,
};

use crate::{game::Game, limits::Limits};

pub struct Shared {
    pub started_at: Instant,
    pub active: AtomicBool,
    pub limits: Limits,
    pub game: Game,
}

impl Default for Shared {
    fn default() -> Self {
        Self { started_at: Instant::now(), active: false.into(), limits: Default::default(), game: Game::default() }
    }
}

impl Shared {
    pub fn set_active(&self, value: bool) {
        if self.active.load(atomic::Ordering::Relaxed) != value {
            self.active.store(value, atomic::Ordering::Relaxed);
        }
    }

    pub fn go(&mut self, started_at: Instant, limits: Limits) {
        self.started_at = started_at;
        *self.active.get_mut() = true;
        self.limits = limits;
    }
}
