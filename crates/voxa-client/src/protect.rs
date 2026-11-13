//! Secrets in settings.json (identity keys, saved passwords) are encrypted with Windows
//! data protection for the current user: other users of the PC, and copies of the file
//! on another PC, cannot read them.
//!
//! Stored as `"dpapi:<base64>"`. Plain values from older versions are still read and
//! get encrypted the next time the settings are saved.

use base64::Engine;
use serde::{Deserialize, Deserializer, Serializer};

const PREFIX: &str = "dpapi:";

#[cfg(windows)]
fn crypt(data: &[u8], protect: bool) -> Option<Vec<u8>> {
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Cryptography::{CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB};
    let input = CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 };
    let mut out = CRYPT_INTEGER_BLOB::default();
    // SAFETY: the input blob points to `data`, which outlives the call; the output
    // buffer is allocated by Windows, copied, then freed with LocalFree.
    unsafe {
        let ok = if protect {
            CryptProtectData(&input, None, None, None, None, CRYPTPROTECT_UI_FORBIDDEN, &mut out)
        } else {
            CryptUnprotectData(&input, None, None, None, None, CRYPTPROTECT_UI_FORBIDDEN, &mut out)
        };
        if ok.is_err() || out.pbData.is_null() {
            return None;
        }
        let v = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
        let _ = LocalFree(Some(HLOCAL(out.pbData as _)));
        Some(v)
    }
}

#[cfg(not(windows))]
fn crypt(_data: &[u8], _protect: bool) -> Option<Vec<u8>> {
    None
}

/// The value as stored: encrypted if possible. Empty stays empty, and a value that is
/// already encrypted (it could not be decrypted on this PC) is kept as it is.
pub fn seal(plain: &str) -> String {
    if plain.is_empty() || plain.starts_with(PREFIX) {
        return plain.to_string();
    }
    match crypt(plain.as_bytes(), true) {
        Some(b) => format!("{PREFIX}{}", base64::engine::general_purpose::STANDARD.encode(b)),
        None => plain.to_string(),
    }
}

/// The plain value. A value that cannot be decrypted (e.g. settings copied from another
/// user or PC) is returned unchanged, so saving does not destroy it.
pub fn open(stored: &str) -> String {
    let Some(b64) = stored.strip_prefix(PREFIX) else { return stored.to_string() };
    base64::engine::general_purpose::STANDARD
        .decode(b64)
        .ok()
        .and_then(|b| crypt(&b, false))
        .and_then(|p| String::from_utf8(p).ok())
        .unwrap_or_else(|| stored.to_string())
}

/// For `#[serde(with = "crate::protect")]` on String fields.
pub fn serialize<S: Serializer>(v: &str, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&seal(v))
}

pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    Ok(open(&String::deserialize(d)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_legacy() {
        let s = seal("correct horse battery staple");
        assert!(s.starts_with(PREFIX), "{s}");
        assert!(!s.contains("horse"));
        assert_eq!(open(&s), "correct horse battery staple");
        // Plain values from older versions still work; empty stays empty.
        assert_eq!(open("plain"), "plain");
        assert_eq!(seal(""), "");
        // Undecryptable values survive a load / save round trip unchanged.
        let foreign = "dpapi:AAAA";
        assert_eq!(seal(&open(foreign)), foreign);
    }

    /// Settings from before themes keep dark / light; new themes round-trip and still
    /// tell older versions dark or light.
    #[test]
    fn theme_setting_is_compatible() {
        use crate::config::{Settings, ThemeKind};
        let old: Settings = serde_json::from_str(r#"{"dark_theme":false}"#).unwrap();
        assert_eq!(old.theme_kind(), ThemeKind::Light);
        let old: Settings = serde_json::from_str(r#"{"dark_theme":true}"#).unwrap();
        assert_eq!(old.theme_kind(), ThemeKind::Dark);
        for k in ThemeKind::ALL {
            let mut s = Settings::default();
            s.set_theme(k);
            let back: Settings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
            assert_eq!((back.theme_kind(), back.dark_theme), (k, k == ThemeKind::Dark));
        }
    }

    #[test]
    fn settings_file_has_no_plain_secrets() {
        let mut st = crate::config::Settings::default();
        st.identities.push(crate::config::Identity::generate("Test"));
        st.bookmarks.push(crate::config::Bookmark { server_password: "srv-secret".into(), channel_password: "chan-secret".into(), ..Default::default() });
        let key = st.identities[0].key.clone();
        let json = serde_json::to_string(&st).unwrap();
        assert!(!json.contains(&key) && !json.contains("srv-secret") && !json.contains("chan-secret"), "{json}");
        let back: crate::config::Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.identities[0].key, key);
        assert_eq!((back.bookmarks[0].server_password.as_str(), back.bookmarks[0].channel_password.as_str()), ("srv-secret", "chan-secret"));
    }
}
