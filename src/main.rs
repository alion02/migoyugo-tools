#![feature(portable_simd)]
#![allow(
    clippy::missing_transmute_annotations, // don't care
)]

use std::{io::stdin, sync::mpsc::channel, thread::spawn};

use crate::protocol::{EngineMsg, UserMsg, send};

pub mod protocol;
pub mod search;
pub mod state;

fn main() {
    send(&EngineMsg::Id { name: Some("Itgo".into()), author: None, version: None });
    let (line_tx, line_rx) = channel();
    spawn(move || {
        for line in stdin().lines().map_while(Result::ok) {
            if line_tx.send(line).is_err() {
                break;
            }
        }
    });
    let Ok(mut msg) = line_rx.recv() else { return };
    // init engine
    loop {
        match ron::from_str::<UserMsg>(&msg) {
            Ok(msg) => todo!(),
            Err(e) => send(&EngineMsg::Error(e.to_string())),
        }
        let Ok(next) = line_rx.recv() else { return };
        msg = next;
    }
}
