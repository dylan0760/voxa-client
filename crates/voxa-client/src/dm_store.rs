//! Direct messages are stored only on this PC, never on the server.
//!
//! One file per server and person under `%APPDATA%\Voxa\dm`, encrypted with
//! ChaCha20-Poly1305. The key is random and kept in `dm\key.txt`, protected with Windows
//! data protection for the current user (see `protect`), so other users of the PC and
//! copies of the files on another PC cannot read the conversations.
//!
//! File: `VXDM1` | 12-byte nonce | ciphertext of the JSON conversation. The server is
//! bound into the encryption (associated data) and every file names its person, so files
//! cannot be moved between servers or people.

use std::path::PathBuf;
use std::sync::OnceLock;

use base64::Engine;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use voxa_proto::ChatMessage;

const MAGIC: &[u8] = b"VXDM1";
/// Messages kept per conversation (the oldest are dropped).
pub const MAX_MESSAGES: usize = 5000;

/// A conversation as stored.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct StoredDm {
    /// The other person's identity.
    pub uid: String,
    /// Their nickname when the conversation was last saved.
    pub name: String,
    pub messages: Vec<ChatMessage>,
}

fn root() -> PathBuf {
    crate::config::config_dir().join("dm")
}

fn hash(parts: &[&str]) -> [u8; 32] {
    let mut h = Sha256::new();
    for p in parts {
        h.update((p.len() as u64).to_le_bytes());
        h.update(p.as_bytes());
    }
    h.finalize().into()
}

fn hex16(b: &[u8; 32]) -> String {
    b[..8].iter().map(|x| format!("{x:02x}")).collect()
}

/// The storage key, created on first use (None if it cannot be read or created).
fn key() -> Option<&'static [u8; 32]> {
    static KEY: OnceLock<Option<[u8; 32]>> = OnceLock::new();
    KEY.get_or_init(|| {
        let path = root().join("key.txt");
        if let Ok(stored) = std::fs::read_to_string(&path) {
            let raw = base64::engine::general_purpose::STANDARD.decode(crate::protect::open(stored.trim())).ok()?;
            return raw.try_into().ok();
        }
        let k: [u8; 32] = rand::random();
        std::fs::create_dir_all(root()).ok()?;
        let sealed = crate::protect::seal(&base64::engine::general_purpose::STANDARD.encode(k));
        std::fs::write(&path, sealed).ok()?;
        Some(k)
    })
    .as_ref()
}

fn server_dir(server_key: &str) -> PathBuf {
    root().join(hex16(&hash(&["server", server_key])))
}

fn file(server_key: &str, uid: &str) -> PathBuf {
    server_dir(server_key).join(format!("{}.vdm", hex16(&hash(&["peer", uid]))))
}

fn aad(server_key: &str) -> [u8; 32] {
    hash(&["voxa-dm", server_key])
}

fn seal(server_key: &str, dm: &StoredDm) -> Option<Vec<u8>> {
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key()?));
    let nonce: [u8; 12] = rand::random();
    let json = serde_json::to_vec(dm).ok()?;
    let ct = cipher.encrypt(Nonce::from_slice(&nonce), Payload { msg: &json, aad: &aad(server_key) }).ok()?;
    Some([MAGIC, &nonce, &ct].concat())
}

/// Decrypts a conversation file of this server, whoever it is with.
fn open_any(server_key: &str, data: &[u8]) -> Option<StoredDm> {
    let rest = data.strip_prefix(MAGIC)?;
    if rest.len() < 12 {
        return None;
    }
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key()?));
    let pt = cipher.decrypt(Nonce::from_slice(&rest[..12]), Payload { msg: &rest[12..], aad: &aad(server_key) }).ok()?;
    serde_json::from_slice::<StoredDm>(&pt).ok()
}

fn unseal(server_key: &str, uid: &str, data: &[u8]) -> Option<StoredDm> {
    open_any(server_key, data).filter(|d| d.uid == uid)
}

/// The stored conversation with `uid` on this server.
pub fn load(server_key: &str, uid: &str) -> Option<StoredDm> {
    if server_key.is_empty() {
        return None;
    }
    unseal(server_key, uid, &std::fs::read(file(server_key, uid)).ok()?)
}

/// Saves a conversation (keeping the newest `MAX_MESSAGES`). An empty one is deleted.
pub fn save(server_key: &str, dm: &StoredDm) -> bool {
    if server_key.is_empty() {
        return false;
    }
    if dm.messages.is_empty() {
        delete(server_key, &dm.uid);
        return true;
    }
    let mut dm = dm.clone();
    if dm.messages.len() > MAX_MESSAGES {
        dm.messages.drain(..dm.messages.len() - MAX_MESSAGES);
    }
    let Some(blob) = seal(server_key, &dm) else { return false };
    let path = file(server_key, &dm.uid);
    if std::fs::create_dir_all(server_dir(server_key)).is_err() {
        return false;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, blob).is_ok() && std::fs::rename(&tmp, &path).is_ok()
}

/// Deletes the stored conversation with `uid` on this server.
pub fn delete(server_key: &str, uid: &str) {
    if !server_key.is_empty() {
        let _ = std::fs::remove_file(file(server_key, uid));
    }
}

/// Everyone you have a stored conversation with on this server: (uid, name, time of the
/// last message), newest first.
pub fn list(server_key: &str) -> Vec<(String, String, u64)> {
    let Ok(rd) = std::fs::read_dir(server_dir(server_key)) else { return Vec::new() };
    let mut out: Vec<(String, String, u64)> = rd
        .flatten()
        .filter(|e| e.path().extension().map(|x| x == "vdm").unwrap_or(false))
        .filter_map(|e| std::fs::read(e.path()).ok())
        .filter_map(|data| open_any(server_key, &data))
        .map(|d| (d.uid, d.name, d.messages.last().map(|m| m.ts).unwrap_or(0)))
        .collect();
    out.sort_by(|a, b| b.2.cmp(&a.2));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use voxa_proto::TextTarget;

    fn msg(text: &str, ts: u64) -> ChatMessage {
        ChatMessage { id: ts, target: TextTarget::Private(0), from: None, from_uid: "AAA".into(), from_name: "Alice".into(), ts, text: text.into() }
    }

    /// Conversations survive a round trip, are unreadable on disk, stay with their server
    /// and person, and can be deleted.
    #[test]
    fn stored_encrypted_per_server_and_person() {
        let dir = std::env::temp_dir().join(format!("voxa-dm-test-{}", std::process::id()));
        std::env::set_var("VOXA_CONFIG_DIR", &dir);
        let dm = StoredDm { uid: "AAA".into(), name: "Alice".into(), messages: vec![msg("secret hello", 1), msg("second", 2)] };
        assert!(save("server-1", &dm));
        let path = file("server-1", "AAA");
        let raw = String::from_utf8_lossy(&std::fs::read(&path).unwrap()).to_string();
        assert!(!raw.contains("secret hello") && !raw.contains("Alice"), "readable on disk");
        assert_eq!(load("server-1", "AAA").unwrap().messages.len(), 2);
        assert!(load("server-2", "AAA").is_none(), "readable under another server");
        // A file copied under another person's name is not accepted for them.
        std::fs::copy(&path, file("server-1", "BBB")).unwrap();
        assert!(load("server-1", "BBB").is_none());
        assert_eq!(list("server-1").iter().filter(|(u, _, _)| u == "AAA").count(), 2);
        delete("server-1", "AAA");
        assert!(load("server-1", "AAA").is_none());
        let key = std::fs::read_to_string(root().join("key.txt")).unwrap();
        assert!(key.starts_with("dpapi:"), "the key file is not protected: {key}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
