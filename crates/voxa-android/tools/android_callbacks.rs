//! Android framework callbacks must belong to JVM classes. Emit three tiny
//! class files containing native declarations, then let the SDK's d8 package
//! them. All capture, permission and service logic lives in projection.rs.
use std::{env, fs, io, path::Path};
fn word(buf: &mut Vec<u8>, n: u16) {
    buf.extend(n.to_be_bytes());
}
fn dword(buf: &mut Vec<u8>, n: u32) {
    buf.extend(n.to_be_bytes());
}
#[derive(Default)]
struct Pool(Vec<Vec<u8>>);
impl Pool {
    fn add(&mut self, tag: u8, bytes: Vec<u8>) -> u16 {
        let mut v = vec![tag];
        v.extend(bytes);
        self.0.push(v);
        self.0.len() as u16
    }
    fn utf(&mut self, s: &str) -> u16 {
        let mut b = Vec::new();
        word(&mut b, s.len() as u16);
        b.extend(s.as_bytes());
        self.add(1, b)
    }
    fn class(&mut self, s: &str) -> u16 {
        let i = self.utf(s);
        self.add(7, i.to_be_bytes().to_vec())
    }
    fn method(&mut self, class: u16, name: &str, desc: &str) -> u16 {
        let mut b = Vec::new();
        word(&mut b, self.utf(name));
        word(&mut b, self.utf(desc));
        let sig = self.add(12, b);
        let mut b = Vec::new();
        word(&mut b, class);
        word(&mut b, sig);
        self.add(10, b)
    }
}
fn class_file(dir: &Path, name: &str, parent: &str, natives: &[(&str, &str)], load: bool) -> io::Result<()> {
    let mut cp = Pool::default();
    let this = cp.class(&format!("io/github/dylan0760/voxa/{name}"));
    let base = cp.class(parent);
    let init = cp.method(base, "<init>", "()V");
    let code = cp.utf("Code");
    let mut methods = Vec::new();
    let mut method = |cp: &mut Pool, flags: u16, name: &str, desc: &str, body: Option<(u16, &[u8])>| {
        word(&mut methods, flags);
        word(&mut methods, cp.utf(name));
        word(&mut methods, cp.utf(desc));
        word(&mut methods, body.is_some() as u16);
        if let Some((locals, bytes)) = body {
            word(&mut methods, code);
            dword(&mut methods, 12 + bytes.len() as u32);
            word(&mut methods, 1);
            word(&mut methods, locals);
            dword(&mut methods, bytes.len() as u32);
            methods.extend(bytes);
            word(&mut methods, 0);
            word(&mut methods, 0);
        }
    };
    let [hi, lo] = init.to_be_bytes();
    method(&mut cp, 1, "<init>", "()V", Some((1, &[0x2a, 0xb7, hi, lo, 0xb1])));
    if load {
        let sys = cp.class("java/lang/System");
        let [hi, lo] = cp.method(sys, "loadLibrary", "(Ljava/lang/String;)V").to_be_bytes();
        let lib = cp.utf("voxa");
        let string = cp.add(8, lib.to_be_bytes().to_vec());
        method(&mut cp, 8, "<clinit>", "()V", Some((0, &[0x12, string as u8, 0xb8, hi, lo, 0xb1])));
    }
    for &(name, desc) in natives {
        method(&mut cp, 0x101, name, desc, None);
    }
    let mut file = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 52];
    word(&mut file, 1 + cp.0.len() as u16);
    for c in cp.0 {
        file.extend(c);
    }
    for v in [0x21, this, base, 0, 0, 1 + load as u16 + natives.len() as u16] {
        word(&mut file, v);
    }
    file.extend(methods);
    word(&mut file, 0);
    fs::write(dir.join(format!("{name}.class")), file)
}
fn main() -> io::Result<()> {
    let path = env::args_os().nth(1).expect("output directory");
    let dir = Path::new(&path);
    fs::create_dir_all(dir)?;
    class_file(dir, "VoxaActivity", "android/app/NativeActivity", &[("onActivityResult", "(IILandroid/content/Intent;)V")], true)?;
    class_file(
        dir,
        "ProjectionService",
        "android/app/Service",
        &[("onBind", "(Landroid/content/Intent;)Landroid/os/IBinder;"), ("onStartCommand", "(Landroid/content/Intent;II)I"), ("onDestroy", "()V")],
        true,
    )?;
    class_file(dir, "ProjectionEnd", "android/media/projection/MediaProjection$Callback", &[("onStop", "()V")], false)
}
