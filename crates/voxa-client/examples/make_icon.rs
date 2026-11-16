//! Regenerates `assets/voxa.ico` and `assets/voxa.png`:
//! `cargo run -p voxa-client --example make_icon`

#[path = "../src/icon.rs"]
mod icon;

use image::codecs::ico::{IcoEncoder, IcoFrame};
use image::ExtendedColorType;

fn main() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    std::fs::create_dir_all(&root).unwrap();
    for (server, name) in [(false, "voxa.ico")] {
        let frames: Vec<IcoFrame> = [256u32, 64, 48, 32, 16]
            .iter()
            .map(|&s| IcoFrame::as_png(&icon::icon_rgba(s, server), s, s, ExtendedColorType::Rgba8).unwrap())
            .collect();
        let f = std::fs::File::create(root.join(name)).unwrap();
        IcoEncoder::new(f).encode_images(&frames).unwrap();
        let png = root.join(name.replace(".ico", ".png"));
        image::save_buffer(&png, &icon::icon_rgba(256, server), 256, 256, ExtendedColorType::Rgba8).unwrap();
        println!("wrote {name}");
    }
    // The Android app's launcher icons.
    let res = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../voxa-android/res");
    for (dir, s) in [("mipmap-mdpi", 48u32), ("mipmap-hdpi", 72), ("mipmap-xhdpi", 96), ("mipmap-xxhdpi", 144), ("mipmap-xxxhdpi", 192)] {
        let d = res.join(dir);
        std::fs::create_dir_all(&d).unwrap();
        image::save_buffer(d.join("ic_launcher.png"), &icon::icon_rgba(s, false), s, s, ExtendedColorType::Rgba8).unwrap();
    }
    println!("wrote the Android icons");
}
