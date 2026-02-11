use std::{
    mem::take,
    ops::Index,
    sync::{Mutex, atomic::AtomicU8},
};

use rand::prelude::*;

pub struct Table {
    pub raw: Box<[Entry]>,
}

impl Table {
    pub fn new(len: usize) -> Self {
        Self { raw: (0..len).map(|_| Entry::default()).collect() }
    }

    pub fn resize(&mut self, new_len: usize) {
        take(&mut self.raw);
        *self = Self::new(new_len);
    }

    pub fn to_index(&self, hash: u64) -> usize {
        ((hash as u128 * self.raw.len() as u128) >> 64) as usize
    }
}

impl Index<u64> for Table {
    type Output = Entry;

    fn index(&self, hash: u64) -> &Self::Output {
        &self.raw[self.to_index(hash)]
    }
}

unsafe impl Send for Table {}
unsafe impl Sync for Table {}

pub struct Entry {
    pub mv: AtomicU8,
}

#[expect(clippy::derivable_impls)]
impl Default for Entry {
    fn default() -> Self {
        Self { mv: 0u8.into() }
    }
}

static mut HASH: [u64; 128] = [0; _];

pub const HASH_STM: u64 = 0x0d0575c6271b1089;

pub(crate) fn init_hash() {
    static INIT: Mutex<bool> = Mutex::new(false);

    let mut init = INIT.lock().unwrap();
    if !*init {
        let rng = &mut rand_chacha::ChaCha20Rng::seed_from_u64(0);
        rng.fill(unsafe { &mut HASH });
        *init = true;
    }
}

pub(crate) fn hash_migo() -> &'static [u64; 64] {
    unsafe { HASH[..64].as_array().unwrap() }
}

pub(crate) fn hash_yugo() -> &'static [u64; 64] {
    unsafe { HASH[64..].as_array().unwrap() }
}
