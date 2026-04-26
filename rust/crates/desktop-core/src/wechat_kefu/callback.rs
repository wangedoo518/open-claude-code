//! WeChat Customer Service callback handling.
//!
//! Self-contained implementation of callback signature verification and
//! AES-256-CBC decryption. Does NOT use `wxkefu_rs::callback::CallbackCrypto`
//! because that library rejects valid EncodingAESKey values that don't end
//! with specific base64 padding chars (a false-negative validation bug).

use base64::Engine;

#[derive(Debug, thiserror::Error)]
pub enum CallbackError {
    #[error("verification failed: {0}")]
    Verification(String),
    #[error("decryption failed: {0}")]
    Decryption(String),
    #[error("parse error: {0}")]
    Parse(String),
    #[error("bad aes key: {0}")]
    BadKey(String),
}

/// Parsed callback event from WeChat.
#[derive(Debug, Clone)]
pub enum CallbackEvent {
    /// New message available — call sync_msg with this token.
    MsgReceive { token: String },
    /// User entered the customer service session.
    EnterSession {
        welcome_code: String,
        scene: Option<String>,
    },
    /// Other event type.
    Other(String),
}

/// Callback handler with self-contained crypto.
pub struct KefuCallback {
    token: String,
    aes_key: [u8; 32],
    #[allow(dead_code)]
    corpid: String,
}

impl KefuCallback {
    pub fn new(token: &str, encoding_aes_key: &str, corpid: &str) -> Result<Self, CallbackError> {
        // EncodingAESKey is 43-char unpadded base64. WeChat generates keys
        // with non-canonical trailing bits (e.g. ending in 'h' where only
        // A/E/I/M/Q/U/Y/c/g/k/o/s/w/0/4/8 are canonical). Rust's base64
        // crate strictly rejects non-canonical symbols, so we use a lenient
        // engine that ignores trailing padding bits.
        let key_str = encoding_aes_key.trim();
        let decoded = {
            use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};
            let lenient_config = GeneralPurposeConfig::new()
                .with_decode_padding_mode(DecodePaddingMode::Indifferent)
                .with_decode_allow_trailing_bits(true);
            let engine = GeneralPurpose::new(&base64::alphabet::STANDARD, lenient_config);
            engine.decode(key_str.as_bytes()).map_err(|e| {
                CallbackError::BadKey(format!("base64 decode failed: {e} (len={})", key_str.len()))
            })?
        };

        if decoded.len() != 32 {
            return Err(CallbackError::BadKey(format!(
                "expected 32 bytes, got {}",
                decoded.len()
            )));
        }

        let mut aes_key = [0u8; 32];
        aes_key.copy_from_slice(&decoded);

        Ok(Self {
            token: token.to_string(),
            aes_key,
            corpid: corpid.to_string(),
        })
    }

    /// Compute SHA1 signature: sha1(sort([token, timestamp, nonce, data]).join(""))
    fn signature(&self, timestamp: &str, nonce: &str, data: &str) -> String {
        let mut parts = [self.token.as_str(), timestamp, nonce, data];
        parts.sort_unstable();
        let joined = parts.concat();

        // WeChat uses SHA1, not SHA256. Use the sha1 crate indirectly via raw computation.
        sha1_hex(joined.as_bytes())
    }

    /// Handle GET callback verification (echostr decryption).
    pub fn verify_echostr(
        &self,
        msg_signature: &str,
        timestamp: &str,
        nonce: &str,
        echostr: &str,
    ) -> Result<String, CallbackError> {
        // 1. Verify signature
        let expected = self.signature(timestamp, nonce, echostr);
        if expected != msg_signature {
            return Err(CallbackError::Verification(format!(
                "signature mismatch: expected={expected}, got={msg_signature}"
            )));
        }

        // 2. Decrypt echostr (it's base64-encoded AES-CBC ciphertext)
        let plaintext = self.decrypt_aes_cbc(echostr)?;

        // 3. Extract the message content (skip 16-byte random + 4-byte length prefix)
        self.extract_content(&plaintext)
    }

    /// Handle POST callback event.
    pub fn decrypt_event(
        &self,
        msg_signature: &str,
        timestamp: &str,
        nonce: &str,
        encrypted_body: &str,
    ) -> Result<CallbackEvent, CallbackError> {
        let encrypt_content = extract_xml_field(encrypted_body, "Encrypt")
            .ok_or_else(|| CallbackError::Parse("missing <Encrypt> field".into()))?;

        // Verify signature
        let expected = self.signature(timestamp, nonce, &encrypt_content);
        if expected != msg_signature {
            return Err(CallbackError::Verification(format!(
                "signature mismatch: expected={expected}, got={msg_signature}"
            )));
        }

        // Decrypt
        let plaintext = self.decrypt_aes_cbc(&encrypt_content)?;
        let content = self.extract_content(&plaintext)?;

        eprintln!(
            "[kefu callback] decrypted content: {}",
            &content[..200.min(content.len())]
        );
        parse_callback_content(&content)
    }

    /// AES-256-CBC decrypt with PKCS7 unpadding.
    /// IV = first 16 bytes of the AES key.
    fn decrypt_aes_cbc(&self, cipher_b64: &str) -> Result<Vec<u8>, CallbackError> {
        let ciphertext = base64::engine::general_purpose::STANDARD
            .decode(cipher_b64.as_bytes())
            .map_err(|e| CallbackError::Decryption(format!("base64 decode: {e}")))?;

        if ciphertext.len() < 16 || ciphertext.len() % 16 != 0 {
            return Err(CallbackError::Decryption(
                "invalid ciphertext length".into(),
            ));
        }

        // IV = first 16 bytes of the AES key
        let iv = &self.aes_key[..16];

        // Decrypt using AES-256-CBC manually with the aes crate
        // Since we have aes-gcm already, use its underlying AES.
        // Actually, let's use raw block cipher operations.
        let mut plaintext = Vec::with_capacity(ciphertext.len());
        let mut prev_block = iv.to_vec();

        for chunk in ciphertext.chunks(16) {
            // ECB decrypt one block
            let decrypted_block = aes_ecb_decrypt_block(&self.aes_key, chunk)?;
            // XOR with previous ciphertext block (CBC)
            let plain_block: Vec<u8> = decrypted_block
                .iter()
                .zip(prev_block.iter())
                .map(|(d, p)| d ^ p)
                .collect();
            plaintext.extend_from_slice(&plain_block);
            prev_block = chunk.to_vec();
        }

        // Remove PKCS7 padding
        if let Some(&pad_len) = plaintext.last() {
            let pad_len = pad_len as usize;
            if pad_len > 0 && pad_len <= 16 && plaintext.len() >= pad_len {
                let valid = plaintext[plaintext.len() - pad_len..]
                    .iter()
                    .all(|&b| b == pad_len as u8);
                if valid {
                    plaintext.truncate(plaintext.len() - pad_len);
                }
            }
        }

        Ok(plaintext)
    }

    /// Extract content from decrypted plaintext:
    /// [16 bytes random][4 bytes msg_len (big-endian)][msg_content][corpid]
    fn extract_content(&self, plaintext: &[u8]) -> Result<String, CallbackError> {
        if plaintext.len() < 20 {
            return Err(CallbackError::Decryption("plaintext too short".into()));
        }

        let msg_len =
            u32::from_be_bytes([plaintext[16], plaintext[17], plaintext[18], plaintext[19]])
                as usize;

        if 20 + msg_len > plaintext.len() {
            return Err(CallbackError::Decryption(format!(
                "msg_len={msg_len} exceeds plaintext len={}",
                plaintext.len()
            )));
        }

        let content = &plaintext[20..20 + msg_len];
        String::from_utf8(content.to_vec())
            .map_err(|e| CallbackError::Decryption(format!("utf8: {e}")))
    }
}

/// AES-256-ECB decrypt a single 16-byte block.
fn aes_ecb_decrypt_block(key: &[u8; 32], block: &[u8]) -> Result<Vec<u8>, CallbackError> {
    use aes_gcm::aead::generic_array::GenericArray;
    use aes_gcm::aes::cipher::{BlockDecrypt, KeyInit};
    use aes_gcm::aes::Aes256;

    let cipher = Aes256::new(GenericArray::from_slice(key));
    let mut output = GenericArray::clone_from_slice(block);
    cipher.decrypt_block(&mut output);
    Ok(output.to_vec())
}

/// Simple SHA1 implementation (WeChat uses SHA1 for callback signatures).
fn sha1_hex(data: &[u8]) -> String {
    // Use a minimal SHA1 implementation
    let digest = sha1_digest(data);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// SHA1 hash (RFC 3174). Minimal implementation for callback signature only.
fn sha1_digest(data: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0];
    let bit_len = (data.len() as u64) * 8;

    // Pad message
    let mut padded = data.to_vec();
    padded.push(0x80);
    while (padded.len() % 64) != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    // Process 64-byte blocks
    for chunk in padded.chunks(64) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }

        let (mut a, mut b, mut c, mut d, mut e) = (h[0], h[1], h[2], h[3], h[4]);

        for i in 0..80 {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A827999u32),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1u32),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDCu32),
                _ => (b ^ c ^ d, 0xCA62C1D6u32),
            };
            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(w[i]);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
    }

    let mut result = [0u8; 20];
    for (i, &val) in h.iter().enumerate() {
        result[i * 4..i * 4 + 4].copy_from_slice(&val.to_be_bytes());
    }
    result
}

fn extract_xml_field(xml: &str, field: &str) -> Option<String> {
    let open = format!("<{field}>");
    let close = format!("</{field}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)? + start;
    let value = xml[start..end].trim();
    if value.starts_with("<![CDATA[") && value.ends_with("]]>") {
        Some(value[9..value.len() - 3].to_string())
    } else {
        Some(value.to_string())
    }
}

fn parse_callback_content(content: &str) -> Result<CallbackEvent, CallbackError> {
    // WeChat kf callback decrypted payload can be JSON or XML.
    // JSON format: {"ToUserName":"corpid","Token":"xxx","OpenKfId":"wk..."}
    // XML format: <xml><Event>...</Event><Token>...</Token></xml>

    // Try JSON first (more common for kf callbacks)
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(content) {
        let token = json
            .get("Token")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();

        if let Some(event) = json.get("Event").and_then(|v| v.as_str()) {
            match event {
                "enter_session" => {
                    let welcome_code = json
                        .get("WelcomeCode")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string();
                    let scene = json
                        .get("Scene")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    return Ok(CallbackEvent::EnterSession {
                        welcome_code,
                        scene,
                    });
                }
                other => return Ok(CallbackEvent::Other(other.to_string())),
            }
        }

        if !token.is_empty() {
            return Ok(CallbackEvent::MsgReceive { token });
        }

        // kf_msg_or_event notification — still has Token
        if json.get("Token").is_some() {
            return Ok(CallbackEvent::MsgReceive { token });
        }

        return Ok(CallbackEvent::Other(
            json.get("MsgType")
                .or(json.get("InfoType"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown_json")
                .to_string(),
        ));
    }

    // Fallback: XML parsing
    let token = extract_xml_field(content, "Token").unwrap_or_default();

    if let Some(event_type) = extract_xml_field(content, "Event") {
        match event_type.as_str() {
            "enter_session" => {
                let welcome_code = extract_xml_field(content, "WelcomeCode").unwrap_or_default();
                let scene = extract_xml_field(content, "Scene");
                return Ok(CallbackEvent::EnterSession {
                    welcome_code,
                    scene,
                });
            }
            // kf_msg_or_event = new message notification, Token is for sync_msg
            "kf_msg_or_event" => {
                if !token.is_empty() {
                    return Ok(CallbackEvent::MsgReceive { token });
                }
            }
            _ => {}
        }
    }

    if !token.is_empty() {
        Ok(CallbackEvent::MsgReceive { token })
    } else {
        Ok(CallbackEvent::Other("unknown".to_string()))
    }
}
