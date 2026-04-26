//! In-memory state for in-flight WeChat QR login flows.
//!
//! Each call to the HTTP route `POST /api/desktop/wechat/login/start`
//! creates a [`PendingLoginSlot`] which lives in
//! [`crate::DesktopState::pending_logins`] for up to 5 minutes while the
//! user scans the QR code on their phone. The slot is polled by the
//! frontend via `GET /api/desktop/wechat/login/{handle}/status` and
//! cleaned up either when the user confirms (the slot transitions to
//! [`PendingLoginState::Confirmed`]), when they cancel it, or when the
//! 5-minute TTL elapses.
//!
//! The types here deliberately hold no references and all state is
//! stored behind `Arc<Mutex<...>>` so a slot can be updated from the
//! background login task while HTTP handlers read the current state.

use std::time::Instant;

use tokio::sync::oneshot;

/// Maximum lifetime of a pending QR login flow. After this the slot is
/// marked [`PendingLoginState::Expired`] on next status poll.
pub const PENDING_LOGIN_TTL_SECS: u64 = 300;

/// The states a QR login flow can be in. Mirrors the Wire protocol used
/// by the frontend status poll.
#[derive(Debug, Clone)]
pub enum PendingLoginState {
    /// QR displayed, user has not yet scanned.
    Waiting,
    /// User scanned on their phone but has not yet confirmed.
    Scanned,
    /// User confirmed; the background task has persisted the account
    /// and started a monitor for it.
    Confirmed {
        /// The normalized account id (filename-safe form, e.g.
        /// `09cf1cc91c42-im-bot`).
        account_id: String,
    },
    /// The login flow failed. `error` is a human-readable message.
    Failed { error: String },
    /// The user or the frontend explicitly cancelled the flow.
    Cancelled,
    /// 5-minute TTL elapsed without a confirmation.
    Expired,
}

impl PendingLoginState {
    /// String discriminant used in the HTTP JSON response.
    #[must_use]
    pub fn wire_tag(&self) -> &'static str {
        match self {
            Self::Waiting => "waiting",
            Self::Scanned => "scanned",
            Self::Confirmed { .. } => "confirmed",
            Self::Failed { .. } => "failed",
            Self::Cancelled => "cancelled",
            Self::Expired => "expired",
        }
    }

    #[must_use]
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Confirmed { .. } | Self::Failed { .. } | Self::Cancelled | Self::Expired
        )
    }
}

/// In-memory record of one in-flight QR login flow.
///
/// `cancel_tx` is `Option<oneshot::Sender<()>>` because the sender is
/// consumed by the `POST /cancel` handler; after cancel it is `None`.
pub struct PendingLoginSlot {
    /// Opaque handle returned to the frontend. Also the hash-map key.
    pub handle: String,
    /// When this slot was created. Used for TTL computation.
    pub created_at: Instant,
    /// Data URI (`data:image/png;base64,...`) for the QR image. Not
    /// strictly required by the iLink protocol (the server returns a
    /// plain https URL in `qrcode_img_content`), but we store whatever
    /// the backend gave us for the frontend to render directly.
    pub qr_image_content: String,
    /// Current status. Mutated by the background login task as the
    /// protocol advances, read by the status poll handler.
    pub state: PendingLoginState,
    /// Send `()` to abort the background login task. Consumed by the
    /// cancel handler.
    pub cancel_tx: Option<oneshot::Sender<()>>,
}

impl PendingLoginSlot {
    /// How long this slot has been alive.
    #[must_use]
    pub fn age_secs(&self) -> u64 {
        self.created_at.elapsed().as_secs()
    }

    /// True if the slot has exceeded [`PENDING_LOGIN_TTL_SECS`] and
    /// should be garbage-collected on the next status poll. Terminal
    /// states (confirmed / failed / cancelled / expired) are NOT
    /// considered "expired" — they are kept briefly so the frontend
    /// can read the final status before the record disappears.
    #[must_use]
    pub fn is_past_ttl(&self) -> bool {
        !matches!(self.state, PendingLoginState::Confirmed { .. })
            && !matches!(self.state, PendingLoginState::Failed { .. })
            && !matches!(self.state, PendingLoginState::Cancelled)
            && !matches!(self.state, PendingLoginState::Expired)
            && self.age_secs() >= PENDING_LOGIN_TTL_SECS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_tags_round_trip() {
        let cases = [
            (PendingLoginState::Waiting, "waiting"),
            (PendingLoginState::Scanned, "scanned"),
            (
                PendingLoginState::Confirmed {
                    account_id: "x".to_string(),
                },
                "confirmed",
            ),
            (
                PendingLoginState::Failed {
                    error: "e".to_string(),
                },
                "failed",
            ),
            (PendingLoginState::Cancelled, "cancelled"),
            (PendingLoginState::Expired, "expired"),
        ];
        for (state, expected) in cases {
            assert_eq!(state.wire_tag(), expected);
        }
    }

    #[test]
    fn terminal_states_are_terminal() {
        assert!(!PendingLoginState::Waiting.is_terminal());
        assert!(!PendingLoginState::Scanned.is_terminal());
        assert!(PendingLoginState::Confirmed {
            account_id: "x".to_string(),
        }
        .is_terminal());
        assert!(PendingLoginState::Failed {
            error: "e".to_string(),
        }
        .is_terminal());
        assert!(PendingLoginState::Cancelled.is_terminal());
        assert!(PendingLoginState::Expired.is_terminal());
    }
}
