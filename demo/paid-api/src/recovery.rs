// SPDX-License-Identifier: MIT

//! Opaque recovery receipts for the stateless Render demo.
//!
//! Render's free filesystem is temporary, so live demo state must travel with the
//! browser and be presented again after a restart. The receipt is authenticated and
//! encrypted with a stable Render environment key; it is never a plaintext state file.

use chacha20poly1305::{XChaCha20Poly1305, XNonce, aead::Aead, aead::KeyInit};

const TOKEN_VERSION: u8 = 1;
const NONCE_LEN: usize = 24;
const MAX_RECOVERY_PAYLOAD: usize = 16 * 1024;
const TOKEN_PREFIX: &str = "glr1.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryError {
    InvalidKey,
    InvalidToken,
    TooLarge,
    Randomness,
}

impl std::fmt::Display for RecoveryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidKey => f.write_str("recovery key must be 32 bytes of hex"),
            Self::InvalidToken => f.write_str("invalid or unauthenticated recovery receipt"),
            Self::TooLarge => f.write_str("recovery state exceeds the 16 KiB limit"),
            Self::Randomness => f.write_str("secure random generation failed"),
        }
    }
}

impl std::error::Error for RecoveryError {}

/// Encrypt and authenticate a recovery payload. The caller must not log the returned token.
pub fn seal(payload: &[u8], key: &[u8; 32]) -> Result<String, RecoveryError> {
    if payload.len() > MAX_RECOVERY_PAYLOAD {
        return Err(RecoveryError::TooLarge);
    }
    let mut nonce = [0u8; NONCE_LEN];
    getrandom::fill(&mut nonce).map_err(|_| RecoveryError::Randomness)?;
    let cipher = XChaCha20Poly1305::new(key.into());
    let mut plaintext = Vec::with_capacity(payload.len() + 1);
    plaintext.push(TOKEN_VERSION);
    plaintext.extend_from_slice(payload);
    let ciphertext = cipher
        .encrypt(XNonce::from_slice(&nonce), plaintext.as_ref())
        .map_err(|_| RecoveryError::InvalidToken)?;
    let mut token = Vec::with_capacity(nonce.len() + ciphertext.len());
    token.extend_from_slice(&nonce);
    token.extend_from_slice(&ciphertext);
    Ok(format!("{TOKEN_PREFIX}{}", hex_encode(&token)))
}

/// Authenticate and decrypt a receipt minted by [`seal`].
pub fn open(token: &str, key: &[u8; 32]) -> Result<Vec<u8>, RecoveryError> {
    let encoded = token
        .strip_prefix(TOKEN_PREFIX)
        .ok_or(RecoveryError::InvalidToken)?;
    if encoded.len() % 2 != 0 || encoded.len() > (MAX_RECOVERY_PAYLOAD + NONCE_LEN + 64) * 2 {
        return Err(RecoveryError::InvalidToken);
    }
    let bytes = hex_decode(encoded).ok_or(RecoveryError::InvalidToken)?;
    if bytes.len() < NONCE_LEN + 17 {
        return Err(RecoveryError::InvalidToken);
    }
    let (nonce, ciphertext) = bytes.split_at(NONCE_LEN);
    let cipher = XChaCha20Poly1305::new(key.into());
    let plaintext = cipher
        .decrypt(XNonce::from_slice(nonce), ciphertext)
        .map_err(|_| RecoveryError::InvalidToken)?;
    if plaintext.first() != Some(&TOKEN_VERSION) {
        return Err(RecoveryError::InvalidToken);
    }
    let payload = plaintext[1..].to_vec();
    if payload.len() > MAX_RECOVERY_PAYLOAD {
        return Err(RecoveryError::TooLarge);
    }
    Ok(payload)
}

/// Parse the dashboard-only `GHOSTLINE_DEMO_STATE_KEY` (exactly 32 bytes, hex).
pub fn key_from_hex(value: &str) -> Result<[u8; 32], RecoveryError> {
    if value.len() != 64 {
        return Err(RecoveryError::InvalidKey);
    }
    let decoded = hex_decode(value).ok_or(RecoveryError::InvalidKey)?;
    decoded.try_into().map_err(|_| RecoveryError::InvalidKey)
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn hex_decode(value: &str) -> Option<Vec<u8>> {
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = (pair[0] as char).to_digit(16)? as u8;
            let low = (pair[1] as char).to_digit(16)? as u8;
            Some((high << 4) | low)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn receipt_is_opaque_and_round_trips() {
        let key = [17u8; 32];
        let payload = br#"{"channel":"test","mask":"never-in-plaintext"}"#;
        let token = seal(payload, &key).unwrap();

        assert!(token.starts_with(TOKEN_PREFIX));
        assert!(!token.contains("never-in-plaintext"));
        assert_eq!(open(&token, &key).unwrap(), payload);
    }

    #[test]
    fn wrong_key_and_tampering_are_rejected() {
        let token = seal(b"channel recovery", &[17u8; 32]).unwrap();
        assert_eq!(open(&token, &[18u8; 32]), Err(RecoveryError::InvalidToken));

        let mut bytes = token.into_bytes();
        let last = bytes.len() - 1;
        bytes[last] = if bytes[last] == b'0' { b'1' } else { b'0' };
        let tampered = String::from_utf8(bytes).unwrap();
        assert_eq!(
            open(&tampered, &[17u8; 32]),
            Err(RecoveryError::InvalidToken)
        );
    }

    #[test]
    fn key_parser_requires_exact_32_byte_hex() {
        assert_eq!(key_from_hex(&"ab".repeat(32)).unwrap(), [0xabu8; 32]);
        assert_eq!(key_from_hex("ab"), Err(RecoveryError::InvalidKey));
        assert_eq!(
            key_from_hex(&"zz".repeat(32)),
            Err(RecoveryError::InvalidKey)
        );
    }

    #[test]
    fn payloads_are_bounded() {
        assert_eq!(
            seal(&vec![0; MAX_RECOVERY_PAYLOAD + 1], &[17u8; 32]),
            Err(RecoveryError::TooLarge)
        );
    }
}
