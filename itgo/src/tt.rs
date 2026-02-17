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

    pub fn clear(&mut self) {
        self.resize(self.raw.len());
    }

    pub fn to_index(&self, hash: u64) -> usize {
        ((hash as u128 * self.raw.len() as u128) >> 64) as usize
    }
}

impl Index<u64> for Table {
    type Output = Entry;

    fn index(&self, hash: u64) -> &Self::Output {
        unsafe { self.raw.get_unchecked(self.to_index(hash)) }
    }
}

unsafe impl Send for Table {}
unsafe impl Sync for Table {}

pub struct Entry {
    pub mv: AtomicU8,
    pub sig: AtomicU8,
}

impl Entry {
    pub const fn new() -> Self {
        Self { mv: AtomicU8::new(0), sig: AtomicU8::new(0) }
    }
}

impl Default for Entry {
    fn default() -> Self {
        Self::new()
    }
}

static mut HASH: [u64; 256] = [0; _];

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

#[derive(Debug, Clone, Copy)]
pub struct SideHash(&'static [u64; 128]);

impl SideHash {
    pub(crate) fn white() -> Self {
        Self(unsafe { HASH[..128].as_array().unwrap() })
    }

    pub(crate) fn black() -> Self {
        Self(unsafe { HASH[128..].as_array().unwrap() })
    }

    pub(crate) fn migo(&self) -> &'static [u64; 64] {
        self.0[..64].as_array().unwrap()
    }

    pub(crate) fn yugo(&self) -> &'static [u64; 64] {
        self.0[64..].as_array().unwrap()
    }
}
