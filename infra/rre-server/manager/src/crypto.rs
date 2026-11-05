//! Wire format shared with the RaceRoom dedicated server.
//!
//! The game server encrypts its state and result callbacks with the constant
//! `DEDI_SECURE_KEY` using RC4 with the first 1024 keystream bytes discarded,
//! and exchanges the payload as uppercase hex inside an `EncryptedMessage`
//! envelope. Both directions must match the C# implementation exactly.

use anyhow::{Context, Result, bail};
use rand::RngCore;

pub const MW_SECURE_KEY: &str = "9A5B19BC7BAA56612E4E75931F9E918F";
pub const DEDI_SECURE_KEY: &str = "d6119e55742b2f6099a911e430cca342";

/// Matches CommunityMultiplayer.Protocols.HexConvertor.ToHexString.
pub fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

/// Matches HexConvertor.ToByteArray, including its uppercase handling.
pub fn hex_decode(value: &str) -> Result<Vec<u8>> {
    if !value.len().is_multiple_of(2) {
        bail!("hex string has an odd length");
    }
    let mut out = Vec::with_capacity(value.len() / 2);
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let high = hex_digit(bytes[index])?;
        let low = hex_digit(bytes[index + 1])?;
        out.push((high << 4) | low);
        index += 2;
    }
    Ok(out)
}

fn hex_digit(byte: u8) -> Result<u8> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => bail!("invalid hex character: {}", byte as char),
    }
}

/// Matches Encryptor.RC4_Drop1024 (discard the first 1024 keystream bytes).
pub fn rc4_drop1024(key: &[u8], data: &[u8]) -> Vec<u8> {
    if key.is_empty() {
        return data.to_vec();
    }
    let mut state = [0u8; 256];
    for (index, value) in state.iter_mut().enumerate() {
        *value = index as u8;
    }
    let mut j = 0usize;
    for index in 0..256 {
        j = (j + state[index] as usize + key[index % key.len()] as usize) & 0xff;
        state.swap(index, j);
    }
    let mut i = 0usize;
    j = 0;
    for _ in 0..1024 {
        i = (i + 1) & 0xff;
        j = (j + state[i] as usize) & 0xff;
        state.swap(i, j);
    }
    let mut out = Vec::with_capacity(data.len());
    for &byte in data {
        i = (i + 1) & 0xff;
        j = (j + state[i] as usize) & 0xff;
        state.swap(i, j);
        let key_byte = state[(state[i] as usize + state[j] as usize) & 0xff];
        out.push(byte ^ key_byte);
    }
    out
}

/// Matches Encryptor.EncryptMessage: random 16-byte key hex plus secureKey,
/// RC4-Drop1024, then the encrypted JSON envelope.
pub fn encrypt_message(message: &str, secure_key: &str) -> Result<Vec<u8>> {
    if message.is_empty() {
        return Ok(Vec::new());
    }
    let mut random_key = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut random_key);
    let key_hex = hex_encode(&random_key);
    let rc4_key = hex_decode(&format!("{key_hex}{secure_key}"))?;
    let data = rc4_drop1024(&rc4_key, message.as_bytes());
    let data_hex = hex_encode(&data);
    let json =
        format!("{{\"EncryptedMessage\":{{\"key\":\"{key_hex}\",\"data\":\"{data_hex}\"}}}}");
    Ok(json.into_bytes())
}

/// Matches Encryptor.DecryptMessage.
pub fn decrypt_message(message_body: &str, key: &str) -> Result<Vec<u8>> {
    if message_body.contains("EncryptedMessage:") {
        bail!("bad encrypted packet");
    }
    if message_body.is_empty() {
        return Ok(Vec::new());
    }
    let normalized: String = message_body
        .chars()
        .filter(|ch| !matches!(ch, ' ' | '"' | '{' | '}'))
        .collect();
    let normalized = normalized.replace("EncryptedMessage:", "");
    let mut parts = normalized.split(',');
    let key_part = parts.next().unwrap_or_default();
    let data_part = parts.next().unwrap_or_default();
    let key_hex = key_part
        .split(':')
        .nth(1)
        .context("missing encrypted key")?;
    let data_hex = data_part
        .split(':')
        .nth(1)
        .context("missing encrypted data")?;
    let rc4_key = hex_decode(&format!("{key_hex}{key}"))?;
    let data = hex_decode(data_hex)?;
    Ok(rc4_drop1024(&rc4_key, &data))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_roundtrip() {
        assert_eq!(hex_encode(b"hello"), "68656C6C6F");
        assert_eq!(hex_encode("日本語".as_bytes()), "E697A5E69CACE8AA9E");
        assert_eq!(hex_decode("68656C6C6F").unwrap(), b"hello");
        assert_eq!(hex_decode("c3bf").unwrap(), vec![0xC3, 0xBF]);
        assert!(hex_decode("xyz").is_err());
        assert!(hex_decode("abc").is_err());
    }

    #[test]
    fn message_roundtrip() {
        let message = "The quick brown fox jumps over the lazy dog";
        let key = "0123456789ABCDEF0123456789ABCDEF";
        let encrypted = encrypt_message(message, key).unwrap();
        let text = String::from_utf8(encrypted).unwrap();
        assert!(text.starts_with("{\"EncryptedMessage\":{\"key\":\""));
        let decrypted = decrypt_message(&text, key).unwrap();
        assert_eq!(decrypted, message.as_bytes());
    }

    #[test]
    fn decrypts_csharp_envelope() {
        // Captured from the C# harness: EncryptMessage(message, key), then the
        // base64-encoded UTF-8 envelope was decoded back to this JSON.
        let cipher = "{\"EncryptedMessage\":{\"key\":\"1A723B4CE7A42A47BA5C80A0E9D3E7E4\",\"data\":\"9F793FFF2C95433947F61BEA5F3F8ADA6909E2B1F516363266749C631DDFDC6652270C6BE72DFFB1E562D4\"}}";
        let key = "0123456789ABCDEF0123456789ABCDEF";
        let plain = decrypt_message(cipher, key).unwrap();
        assert_eq!(
            String::from_utf8(plain).unwrap(),
            "The quick brown fox jumps over the lazy dog"
        );
    }
}
