//! Hardware tokens for hardware bans (servers 1.16.0+).
//!
//! The machine identifiers are the MAC addresses of this PC's physical network adapters
//! and the Windows machine id. They never leave the PC: each is hashed together with
//! the server's fingerprint (`voxa_proto::hardware_token`), so a server only sees tokens
//! that mean nothing to other servers.

use std::sync::OnceLock;

/// This PC's machine identifiers (computed once).
fn identifiers() -> &'static Vec<String> {
    static IDS: OnceLock<Vec<String>> = OnceLock::new();
    IDS.get_or_init(|| {
        let mut ids: Vec<String> = imp::macs().into_iter().map(|m| format!("mac:{m}")).collect();
        ids.sort();
        ids.truncate(6);
        if let Some(g) = imp::machine_guid() {
            ids.push(format!("machine:{}", g.to_ascii_lowercase()));
        }
        ids
    })
}

/// The tokens to give the server with this fingerprint.
pub fn tokens(server_fingerprint: &str) -> Vec<String> {
    identifiers().iter().map(|id| voxa_proto::hardware_token(server_fingerprint, id)).collect()
}

/// A MAC address worth using: 6 bytes, not all zero, not multicast and not "locally
/// administered" (randomised Wi-Fi addresses and most virtual adapters set that bit).
fn usable_mac(m: &[u8]) -> bool {
    m.len() == 6 && m.iter().any(|b| *b != 0) && m[0] & 0x01 == 0 && m[0] & 0x02 == 0
}

#[cfg(windows)]
mod imp {
    use windows::core::w;
    use windows::Win32::NetworkManagement::IpHelper::*;
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RRF_SUBKEY_WOW6464KEY};

    /// MAC addresses of physical Ethernet and Wi-Fi adapters.
    pub fn macs() -> Vec<String> {
        const IF_ETHERNET: u32 = 6;
        const IF_WIFI: u32 = 71;
        let mut out = Vec::new();
        let flags = GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_DNS_SERVER;
        let mut size = 16 * 1024u32;
        for _ in 0..3 {
            let mut buf = vec![0u64; size as usize / 8 + 1];
            let first = buf.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH;
            let rc = unsafe { GetAdaptersAddresses(0, flags, None, Some(first), &mut size) };
            if rc == 111 {
                continue; // ERROR_BUFFER_OVERFLOW: `size` now says how much is needed.
            }
            if rc != 0 {
                return out;
            }
            let mut p = first as *const IP_ADAPTER_ADDRESSES_LH;
            while !p.is_null() {
                let a = unsafe { &*p };
                let len = (a.PhysicalAddressLength as usize).min(8);
                let mac = &a.PhysicalAddress[..len];
                let desc = unsafe { a.Description.to_string() }.unwrap_or_default().to_ascii_lowercase();
                let virtual_adapter = ["virtual", "hyper-v", "vmware", "virtualbox", "vpn", "tap-", "wan miniport", "loopback", "bluetooth", "wireguard", "tunnel"]
                    .iter()
                    .any(|v| desc.contains(v));
                if (a.IfType == IF_ETHERNET || a.IfType == IF_WIFI) && !virtual_adapter && super::usable_mac(mac) {
                    out.push(mac.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(":"));
                }
                p = a.Next;
            }
            break;
        }
        out.sort();
        out.dedup();
        out
    }

    /// The Windows machine id (set when Windows is installed).
    pub fn machine_guid() -> Option<String> {
        let mut buf = [0u16; 64];
        let mut bytes = (buf.len() * 2) as u32;
        let rc = unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                w!("SOFTWARE\\Microsoft\\Cryptography"),
                w!("MachineGuid"),
                RRF_RT_REG_SZ | RRF_SUBKEY_WOW6464KEY,
                None,
                Some(buf.as_mut_ptr() as *mut _),
                Some(&mut bytes),
            )
        };
        if rc.0 != 0 {
            return None;
        }
        let len = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
        let s = String::from_utf16_lossy(&buf[..len]).trim().to_string();
        (s.len() >= 16).then_some(s)
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn macs() -> Vec<String> {
        Vec::new()
    }
    pub fn machine_guid() -> Option<String> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_per_server_and_hide_the_identifiers() {
        assert!(usable_mac(&[0x00, 0x1a, 0x2b, 0x3c, 0x4d, 0x5e]));
        assert!(!usable_mac(&[0x02, 0x1a, 0x2b, 0x3c, 0x4d, 0x5e]), "locally administered");
        assert!(!usable_mac(&[0x01, 0, 0, 0, 0, 1]), "multicast");
        assert!(!usable_mac(&[0; 6]));
        let a = tokens("AAAA:BBBB");
        let b = tokens("CCCC:DDDD");
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(&b) {
            assert_ne!(x, y, "tokens must differ per server");
            assert!(voxa_proto::valid_hardware_token(x));
        }
        // Nothing of the raw identifiers appears in a token.
        for id in identifiers() {
            let raw = id.split(':').skip(1).collect::<String>();
            assert!(a.iter().all(|t| !t.contains(&raw.to_ascii_lowercase()[..raw.len().min(8)])));
        }
        println!("this PC has {} hardware identifiers", identifiers().len());
    }
}
