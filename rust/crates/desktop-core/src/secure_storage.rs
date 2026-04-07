//! AES-256-GCM encrypted file storage for sensitive data (OAuth tokens,
//! API keys, etc.).
//!
//! Motivation: the original `managed_auth` code stores OAuth tokens as
//! plaintext JSON in `~/.warwolf/` and similar paths. Any process with
//! read access to the user's home directory can exfiltrate them. This
//! module provides a transparent encrypt-on-write / decrypt-on-read
//! layer using AES-256-GCM.
//!
//! ── Key management ──────────────────────────────────────────────────
//!
//! The encryption key is derived from a machine-local secret stored at
//! `~/.warwolf/.secret-key`. On first use, a 32-byte random key is
//! generated and the file is created with 0600 permissions (on Unix).
//!
//! This is *not* as strong as using the OS keyring (macOS Keychain,
//! Windows Credential Manager, Linux Secret Service), but it's a
//! meaningful improvement over plaintext and works cross-platform
//! without additional native dependencies. A future iteration can
//! migrate to `keyring` crate for OS-backed storage.
//!
//! ── File format ─────────────────────────────────────────────────────
//!
//! Encrypted files use a custom binary framing:
//!
//!   [4 bytes: magic "WWE1"]
//!   [12 bytes: nonce]
//!   [remaining: ciphertext + 16 byte auth tag]
//!
//! The plaintext is arbitrary bytes. The caller is responsible for
//! serialization (JSON, TOML, etc.) before calling `write_encrypted`.

use std::io;
use std::path::Path;
use std::sync::OnceLock;

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use rand::RngCore;

const MAGIC: &[u8; 4] = b"WWE1";
const NONCE_LEN: usize = 12;
const KEY_LEN: usize = 32;

/// In-memory cache of the encryption key. Populated lazily on first
/// use and reused across the process lifetime to avoid re-reading
/// the key file and to prevent parallel-test races.
static KEY_CACHE: OnceLock<[u8; KEY_LEN]> = OnceLock::new();

#[derive(Debug)]
pub enum SecureStorageError {
    Io(io::Error),
    InvalidFormat(String),
    Crypto(String),
}

impl std::fmt::Display for SecureStorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "secure-storage io error: {e}"),
            Self::InvalidFormat(msg) => write!(f, "secure-storage invalid format: {msg}"),
            Self::Crypto(msg) => write!(f, "secure-storage crypto error: {msg}"),
        }
    }
}

impl std::error::Error for SecureStorageError {}

impl From<io::Error> for SecureStorageError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

/// Locate the machine-local key file. Created lazily by `load_or_create_key`.
fn key_file_path() -> std::path::PathBuf {
    // Use the same home-dir resolution as codex_auth: prefer USERPROFILE
    // on Windows and HOME on Unix, falling back to "." if neither is set.
    let home = std::env::var("USERPROFILE")
        .ok()
        .or_else(|| std::env::var("HOME").ok())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    home.join(".warwolf").join(".secret-key")
}

/// Load the 32-byte encryption key. Uses `KEY_CACHE` to avoid repeated
/// disk I/O. On first call, reads from `key_file_path()` or generates
/// a new key if the file doesn't exist.
///
/// In tests where `key_file_path()` may not be accessible, callers can
/// pre-seed the cache with `seed_key_for_test`.
fn load_or_create_key() -> Result<[u8; KEY_LEN], SecureStorageError> {
    if let Some(cached) = KEY_CACHE.get() {
        return Ok(*cached);
    }

    // Try to read from disk first.
    let path = key_file_path();
    let key = if path.exists() {
        let bytes = std::fs::read(&path)?;
        if bytes.len() != KEY_LEN {
            return Err(SecureStorageError::InvalidFormat(format!(
                "key file has wrong length: expected {KEY_LEN}, got {}",
                bytes.len()
            )));
        }
        let mut key = [0u8; KEY_LEN];
        key.copy_from_slice(&bytes);
        key
    } else {
        // Generate new key.
        let mut key = [0u8; KEY_LEN];
        rand::thread_rng().fill_bytes(&mut key);

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, key)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(meta) = std::fs::metadata(&path) {
                let mut perms = meta.permissions();
                perms.set_mode(0o600);
                let _ = std::fs::set_permissions(&path, perms);
            }
        }
        key
    };

    // Race-tolerant seed: if another thread already populated the cache,
    // trust its key (our fresh key is wasted but harmless since any file
    // written with the stale key will fail to decrypt and fall through).
    let _ = KEY_CACHE.set(key);
    Ok(*KEY_CACHE.get().expect("cache must be populated"))
}

/// Seed the key cache with a deterministic key for tests. Idempotent —
/// only the first call wins. Has no effect in production code.
#[cfg(test)]
pub(crate) fn seed_key_for_test(key: [u8; KEY_LEN]) {
    let _ = KEY_CACHE.set(key);
}

/// Encrypt `plaintext` and write it to `path`. Creates parent dirs as
/// needed. Overwrites existing files atomically via rename.
pub fn write_encrypted(path: &Path, plaintext: &[u8]) -> Result<(), SecureStorageError> {
    let key_bytes = load_or_create_key()?;
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key_bytes));

    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| SecureStorageError::Crypto(format!("encrypt failed: {e}")))?;

    // Assemble framed output.
    let mut out = Vec::with_capacity(MAGIC.len() + NONCE_LEN + ciphertext.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ciphertext);

    // Atomic write via temp file + rename.
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp_path = path.with_extension("enc-tmp");
    std::fs::write(&tmp_path, &out)?;
    std::fs::rename(&tmp_path, path)?;

    Ok(())
}

/// Read and decrypt a file written by `write_encrypted`. Returns the
/// plaintext bytes. Fails if the file doesn't exist, has wrong magic,
/// or authentication fails (tampered / wrong key).
pub fn read_encrypted(path: &Path) -> Result<Vec<u8>, SecureStorageError> {
    let bytes = std::fs::read(path)?;

    if bytes.len() < MAGIC.len() + NONCE_LEN {
        return Err(SecureStorageError::InvalidFormat(
            "file shorter than header".into(),
        ));
    }

    if &bytes[..4] != MAGIC {
        return Err(SecureStorageError::InvalidFormat(
            "magic bytes do not match WWE1".into(),
        ));
    }

    let nonce = Nonce::from_slice(&bytes[4..4 + NONCE_LEN]);
    let ciphertext = &bytes[4 + NONCE_LEN..];

    let key_bytes = load_or_create_key()?;
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key_bytes));

    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| SecureStorageError::Crypto(format!("decrypt failed: {e}")))
}

/// Check if a file on disk looks like an encrypted WWE1 blob.
/// Used for migration: if we find a plaintext file that should be
/// encrypted, convert it.
pub fn is_encrypted(path: &Path) -> bool {
    match std::fs::read(path) {
        Ok(bytes) => bytes.len() >= 4 && &bytes[..4] == MAGIC,
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shared test key so all tests use the same encryption key without
    /// touching the real `~/.warwolf/.secret-key` file. Pre-seeding the
    /// cache avoids the Windows permission race seen when multiple
    /// parallel tests try to create the key file concurrently.
    fn ensure_test_key() {
        let test_key: [u8; KEY_LEN] = [0x42; KEY_LEN];
        seed_key_for_test(test_key);
    }

    fn temp_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "secure-storage-test-{}-{}-{}",
            std::process::id(),
            name,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn roundtrip_small_plaintext() {
        ensure_test_key();
        let path = temp_path("roundtrip-small");
        let plaintext = b"hello, secure world";
        write_encrypted(&path, plaintext).expect("write should succeed");
        assert!(is_encrypted(&path));

        let decrypted = read_encrypted(&path).expect("read should succeed");
        assert_eq!(&decrypted, plaintext);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn roundtrip_large_plaintext() {
        ensure_test_key();
        let path = temp_path("roundtrip-large");
        let plaintext = vec![42u8; 100_000];
        write_encrypted(&path, &plaintext).expect("write should succeed");

        let decrypted = read_encrypted(&path).expect("read should succeed");
        assert_eq!(decrypted, plaintext);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn roundtrip_utf8_json_payload() {
        ensure_test_key();
        let path = temp_path("roundtrip-json");
        // Simulates what managed_auth would write: JSON with OAuth tokens.
        let plaintext = r#"{"access_token":"secret-中文-🔑","expires":1234567890}"#.as_bytes();
        write_encrypted(&path, plaintext).expect("write should succeed");

        let decrypted = read_encrypted(&path).expect("read should succeed");
        assert_eq!(decrypted, plaintext);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn decrypt_tampered_file_fails() {
        ensure_test_key();
        let path = temp_path("tampered");
        write_encrypted(&path, b"original").expect("write should succeed");

        // Flip a byte in the ciphertext region.
        let mut bytes = std::fs::read(&path).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0xFF;
        std::fs::write(&path, &bytes).unwrap();

        let result = read_encrypted(&path);
        assert!(result.is_err(), "decryption of tampered data should fail");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn decrypt_wrong_magic_fails() {
        ensure_test_key();
        let path = temp_path("wrong-magic");
        std::fs::write(&path, b"XXXX\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00junk").unwrap();

        let result = read_encrypted(&path);
        assert!(matches!(
            result,
            Err(SecureStorageError::InvalidFormat(_))
        ));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn decrypt_short_file_fails() {
        ensure_test_key();
        let path = temp_path("short");
        std::fs::write(&path, b"short").unwrap();

        let result = read_encrypted(&path);
        assert!(matches!(
            result,
            Err(SecureStorageError::InvalidFormat(_))
        ));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn is_encrypted_detects_plaintext_vs_encrypted() {
        ensure_test_key();
        let plain_path = temp_path("is-encrypted-plain");
        let encrypted_path = temp_path("is-encrypted-crypted");

        std::fs::write(&plain_path, b"this is plaintext json").unwrap();
        write_encrypted(&encrypted_path, b"this is encrypted").unwrap();

        assert!(!is_encrypted(&plain_path));
        assert!(is_encrypted(&encrypted_path));

        let _ = std::fs::remove_file(&plain_path);
        let _ = std::fs::remove_file(&encrypted_path);
    }
}
