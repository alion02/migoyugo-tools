use std::{
    f64::consts::E,
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

    pub fn go(&mut self, started_at: Instant, mut limits: Limits) {
        if let Some(clock) = &limits.clock {
            const K: f64 = 0.05;
            const EXPECTED_LEN: i32 = 50;
            const EXPECTED_DELAY: f64 = 10.;
            const MIN_TIME: f64 = 10.;
            let expected_moves_left = (1. + E.powf((EXPECTED_LEN - self.game.frame_ptr().ply / 2) as f64 * K)).ln() / K;
            let target_time_fraction = 1. / expected_moves_left.max(1.);
            let time_left = clock.left as f64 - EXPECTED_DELAY;
            let expected_time_available = time_left + clock.incr as f64 * expected_moves_left.floor();
            let computed_limit = (expected_time_available * target_time_fraction).min(time_left).max(MIN_TIME) as u64;
            limits.time = limits.time.min(computed_limit);
        }
        self.started_at = started_at;
        *self.active.get_mut() = true;
        self.limits = limits;
    }

    pub fn reset(&mut self) {
        self.game.undo_all();
        self.tt.clear();
    }
}
