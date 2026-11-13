//! Capture self-test: `cargo run -p voxa-client --example capture_test -- [window-title] [out.png]`
//! Lists capture sources, checks that monitor capture returns real pixels (without
//! saving them) and saves a capture of the named window.

#[path = "../src/capture.rs"]
#[allow(dead_code)]
mod capture;
#[path = "../src/dda.rs"]
#[allow(dead_code)]
mod dda;

fn main() {
    let title = std::env::args().nth(1).unwrap_or("Voxa".into());
    let out = std::env::args().nth(2).unwrap_or("window.png".into());
    let sources = capture::sources();
    let monitors = sources.iter().filter(|s| matches!(s, capture::Source::Monitor { .. })).count();
    println!("{} sources ({} monitors, {} windows)", sources.len(), monitors, sources.len() - monitors);
    if let Some(m) = sources.iter().find(|s| matches!(s, capture::Source::Monitor { .. })) {
        let t = std::time::Instant::now();
        let c = capture::capture(m).expect("monitor capture failed");
        let distinct: std::collections::HashSet<[u8; 3]> = c.bgra.chunks_exact(4).step_by(97).map(|p| [p[0], p[1], p[2]]).collect();
        println!("monitor capture {}x{} in {:?}, {} distinct sampled colours", c.w, c.h, t.elapsed(), distinct.len());
    }
    let w = sources.iter().find(|s| matches!(s, capture::Source::Window { title: t, .. } if t.starts_with(&title)));
    match w {
        Some(src) => {
            let c = capture::capture(src).expect("window capture failed");
            let rgba: Vec<u8> = c.bgra.chunks_exact(4).flat_map(|p| [p[2], p[1], p[0], 255]).collect();
            image::save_buffer(&out, &rgba, c.w, c.h, image::ExtendedColorType::Rgba8).unwrap();
            println!("window \"{}\" captured {}x{} -> {out}", src.label(), c.w, c.h);
        }
        None => println!("window starting with \"{title}\" not found"),
    }
}
