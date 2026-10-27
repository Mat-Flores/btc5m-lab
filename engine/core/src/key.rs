//! Wallet private key, encrypted at rest in the operator's format (`v1.<salt>.<iv>.<tag>.<ciphertext>`: scrypt
//! N=2^17 r=8 p=1, AES-256-GCM, base64url; the current application's `src/exec/wallets.js`). Decrypted in memory
//! only, at arming; every intermediate holding key material is wiped when dropped. Never written, logged or shown.
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use zeroize::Zeroizing;

#[derive(Debug, PartialEq, Eq)]
pub enum KeyError { Format, Passphrase }

/// The 32-byte private key, or an error that says nothing about the key.
pub fn decrypt_v1(payload: &str, passphrase: &str) -> Result<Zeroizing<[u8; 32]>, KeyError> {
    let p: Vec<&str> = payload.trim().split('.').collect();
    let [v, salt, iv, tag, ct] = p[..] else { return Err(KeyError::Format) };
    if v != "v1" { return Err(KeyError::Format); }
    let b = |s: &str| URL_SAFE_NO_PAD.decode(s.trim_end_matches('=')).map_err(|_| KeyError::Format);
    let (salt, iv, tag, ct) = (b(salt)?, b(iv)?, b(tag)?, b(ct)?);
    if iv.len() != 12 || tag.len() != 16 { return Err(KeyError::Format); }
    let mut k = Zeroizing::new([0u8; 32]);
    scrypt::scrypt(passphrase.as_bytes(), &salt, &scrypt::Params::new(17, 8, 1, 32).map_err(|_| KeyError::Format)?, &mut k[..]).map_err(|_| KeyError::Format)?;
    let mut sealed = ct;
    sealed.extend_from_slice(&tag);
    let plain = Zeroizing::new(Aes256Gcm::new_from_slice(&k[..]).map_err(|_| KeyError::Format)?.decrypt(Nonce::from_slice(&iv), sealed.as_ref()).map_err(|_| KeyError::Passphrase)?);
    // the plaintext is the key as typed: 64 hex digits, with or without 0x
    let txt = std::str::from_utf8(&plain).map_err(|_| KeyError::Format)?.trim();
    let hex = txt.strip_prefix("0x").unwrap_or(txt);
    if hex.len() != 64 { return Err(KeyError::Format); }
    let mut out = Zeroizing::new([0u8; 32]);
    for (i, o) in out.iter_mut().enumerate() { *o = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).map_err(|_| KeyError::Format)?; }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vector() -> serde_json::Value {
        serde_json::from_str(&std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../fixtures/sign/key-v1.json")).unwrap()).unwrap()
    }

    #[test]
    fn decrypts_the_current_application_format() {
        let v = vector();
        let pass = v["passphrase"].as_str().unwrap();
        for p in ["payload", "payload_bare"] { assert_eq!(*decrypt_v1(v[p].as_str().unwrap(), pass).unwrap(), [0x11u8; 32]); }
    }

    #[test]
    fn wrong_passphrase_tampering_and_garbage_are_refused() {
        let v = vector();
        let payload = v["payload"].as_str().unwrap();
        assert_eq!(decrypt_v1(payload, "autre phrase"), Err(KeyError::Passphrase));
        // one byte of the ciphertext changed: the GCM tag refuses it
        let mut parts: Vec<String> = payload.split('.').map(String::from).collect();
        let mut ct = URL_SAFE_NO_PAD.decode(&parts[4]).unwrap();
        ct[0] ^= 1;
        parts[4] = URL_SAFE_NO_PAD.encode(ct);
        assert_eq!(decrypt_v1(&parts.join("."), v["passphrase"].as_str().unwrap()), Err(KeyError::Passphrase));
        for bad in ["", "v2.a.b.c.d", "v1.a.b.c", "v1.!.!.!.!"] { assert_eq!(decrypt_v1(bad, "x"), Err(KeyError::Format), "{bad}"); }
    }
}
