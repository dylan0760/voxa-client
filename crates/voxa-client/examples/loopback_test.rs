//! Screen share sound capture self-test:
//! `cargo run -p voxa-client --example loopback_test -- <pid | all> [seconds]`
//! Captures one process (or everything except this program) and prints how much sound
//! arrived and how loud it was. Nothing is stored.

#[path = "../src/loopback.rs"]
#[allow(dead_code)]
mod loopback;

use std::sync::{Arc, Mutex};

fn main() {
    let target = std::env::args().nth(1).unwrap_or("all".into());
    let secs: u64 = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(3);
    let t = match target.parse::<u32>() {
        Ok(pid) => loopback::Target::Process(pid),
        Err(_) => loopback::Target::AllExceptVoxa,
    };
    let stats = Arc::new(Mutex::new((0usize, 0f32)));
    let s2 = stats.clone();
    let cap = loopback::start(t, move |pcm| {
        let mut s = s2.lock().unwrap();
        s.0 += pcm.len() / 2;
        s.1 = pcm.iter().fold(s.1, |m, x| m.max(x.abs()));
    });
    match cap {
        Err(e) => println!("capture failed: {e}"),
        Ok(c) => {
            std::thread::sleep(std::time::Duration::from_secs(secs));
            drop(c);
            let (frames, peak) = *stats.lock().unwrap();
            println!("captured {:.2} s of audio, peak {:.5} ({:.1} dBFS)", frames as f32 / 48_000.0, peak, 20.0 * peak.max(1e-9).log10());
        }
    }
}
