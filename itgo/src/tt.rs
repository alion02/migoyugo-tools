use std::{
    array,
    mem::take,
    ops::Index,
    sync::{Mutex, atomic::AtomicU32},
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

#[repr(align(16))]
pub struct Entry {
    pub raw: [AtomicU32; 4],
}

impl Default for Entry {
    fn default() -> Self {
        Self { raw: array::from_fn(|_| Packed::default().raw().into()) }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Packed(u32);

impl Packed {
    pub fn new(mv: u8, signature: u32, depth: u32, generation: u8) -> Self {
        Self((mv as u32) | signature | depth << 16 | (generation as u32) << 24)
    }

    pub fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    pub fn mv(self) -> u8 {
        self.0 as u8 & 0x3F
    }

    pub fn signature_matches(self, signature: u32) -> bool {
        self.0 & to_signature(!0) == signature
    }

    pub fn depth(self) -> u32 {
        self.0 >> 16 & 0xFF
    }

    pub fn generation(self) -> u8 {
        (self.0 >> 24) as u8
    }

    pub fn raw(self) -> u32 {
        self.0
    }
}

#[expect(clippy::derivable_impls)]
impl Default for Packed {
    fn default() -> Self {
        Self(Default::default())
    }
}

pub fn to_signature(hash: u64) -> u32 {
    (hash as u32 & ((1 << 10) - 1)) << 6
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
