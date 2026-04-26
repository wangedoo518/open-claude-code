//! WebSocket client connecting to the Cloudflare Worker relay.
//! Receives raw callback events, decrypts them, and feeds
//! CallbackEvent into the monitor's callback_rx channel.

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::callback::{CallbackEvent, KefuCallback};

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);
const RECONNECT_BASE: Duration = Duration::from_secs(3);
const RECONNECT_MAX: Duration = Duration::from_secs(25);

pub struct RelayClient {
    relay_url: String,
    auth_token: String,
    callback_token: String,
    encoding_aes_key: String,
    corpid: String,
}

impl RelayClient {
    pub fn new(
        relay_url: &str,
        auth_token: &str,
        callback_token: &str,
        encoding_aes_key: &str,
        corpid: &str,
    ) -> Self {
        Self {
            relay_url: relay_url.to_string(),
            auth_token: auth_token.to_string(),
            callback_token: callback_token.to_string(),
            encoding_aes_key: encoding_aes_key.to_string(),
            corpid: corpid.to_string(),
        }
    }

    /// Main loop: connect → read → dispatch → reconnect on failure.
    pub async fn run(&self, callback_tx: mpsc::Sender<CallbackEvent>, cancel: CancellationToken) {
        let mut backoff = RECONNECT_BASE;

        loop {
            if cancel.is_cancelled() {
                break;
            }

            let url = format!("{}?auth={}", self.relay_url, self.auth_token);
            eprintln!("[relay_client] connecting to {}...", self.relay_url);

            match tokio_tungstenite::connect_async(&url).await {
                Ok((stream, _)) => {
                    backoff = RECONNECT_BASE; // reset on success
                    eprintln!("[relay_client] connected");

                    let (write, read) = stream.split();
                    let write = std::sync::Arc::new(tokio::sync::Mutex::new(write));

                    self.read_loop(read, write, &callback_tx, &cancel).await;

                    eprintln!("[relay_client] disconnected");
                }
                Err(e) => {
                    eprintln!("[relay_client] connect error: {e}");
                }
            }

            if cancel.is_cancelled() {
                break;
            }

            eprintln!("[relay_client] reconnecting in {}ms", backoff.as_millis());
            tokio::select! {
                _ = tokio::time::sleep(backoff) => {}
                _ = cancel.cancelled() => break,
            }
            backoff = (backoff.mul_f64(1.5)).min(RECONNECT_MAX);
        }

        eprintln!("[relay_client] stopped");
    }

    async fn read_loop(
        &self,
        mut read: futures_util::stream::SplitStream<
            tokio_tungstenite::WebSocketStream<
                tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
            >,
        >,
        write: std::sync::Arc<
            tokio::sync::Mutex<
                futures_util::stream::SplitSink<
                    tokio_tungstenite::WebSocketStream<
                        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
                    >,
                    tokio_tungstenite::tungstenite::Message,
                >,
            >,
        >,
        callback_tx: &mpsc::Sender<CallbackEvent>,
        cancel: &CancellationToken,
    ) {
        let mut heartbeat = tokio::time::interval(HEARTBEAT_INTERVAL);
        heartbeat.tick().await;

        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,

                _ = heartbeat.tick() => {
                    let ping = serde_json::json!({"type":"ping"}).to_string();
                    let mut w = write.lock().await;
                    if w.send(tokio_tungstenite::tungstenite::Message::Text(ping)).await.is_err() {
                        break;
                    }
                }

                msg = read.next() => {
                    match msg {
                        Some(Ok(tokio_tungstenite::tungstenite::Message::Text(text))) => {
                            self.handle_relay_message(&text, callback_tx).await;
                        }
                        Some(Ok(tokio_tungstenite::tungstenite::Message::Close(_))) => break,
                        Some(Err(e)) => {
                            eprintln!("[relay_client] read error: {e}");
                            break;
                        }
                        None => break,
                        _ => {}
                    }
                }
            }
        }
    }

    async fn handle_relay_message(&self, text: &str, callback_tx: &mpsc::Sender<CallbackEvent>) {
        let parsed: serde_json::Value = match serde_json::from_str(text) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("[relay_client] bad JSON: {e}");
                return;
            }
        };

        let msg_type = parsed["type"].as_str().unwrap_or("");
        match msg_type {
            "callback" => {
                let params_str = parsed["params"].as_str().unwrap_or("");
                let body = parsed["body"].as_str().unwrap_or("");

                // Parse query params from "?msg_signature=...&timestamp=...&nonce=..."
                let params = parse_query_string(params_str);
                let msg_sig = params
                    .get("msg_signature")
                    .map(|s| s.as_str())
                    .unwrap_or("");
                let timestamp = params.get("timestamp").map(|s| s.as_str()).unwrap_or("");
                let nonce = params.get("nonce").map(|s| s.as_str()).unwrap_or("");

                let callback = match KefuCallback::new(
                    &self.callback_token,
                    &self.encoding_aes_key,
                    &self.corpid,
                ) {
                    Ok(cb) => cb,
                    Err(e) => {
                        eprintln!("[relay_client] KefuCallback::new failed: {e}");
                        return;
                    }
                };

                match callback.decrypt_event(msg_sig, timestamp, nonce, body) {
                    Ok(event) => {
                        eprintln!("[relay_client] event: {event:?}");
                        let _ = callback_tx.send(event).await;
                    }
                    Err(e) => {
                        eprintln!("[relay_client] decrypt failed: {e}");
                    }
                }
            }
            "pong" => {
                // Heartbeat response, ignore
            }
            other => {
                eprintln!("[relay_client] unknown type: {other}");
            }
        }
    }
}

fn parse_query_string(qs: &str) -> std::collections::HashMap<String, String> {
    let qs = qs.strip_prefix('?').unwrap_or(qs);
    qs.split('&')
        .filter_map(|pair| {
            let mut parts = pair.splitn(2, '=');
            let key = parts.next()?;
            let value = parts.next().unwrap_or("");
            Some((
                urlencoding::decode(key).unwrap_or_default().to_string(),
                urlencoding::decode(value).unwrap_or_default().to_string(),
            ))
        })
        .collect()
}
