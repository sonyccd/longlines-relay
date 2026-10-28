//! Device pairing (contract section 4): the sign-in state machine.
//!
//! [`start`] asks Long Lines for a code; [`wait_for_token`] polls until the web user allows or
//! denies it, the code expires, or the caller drops the future (Cancel). The engine runs both and
//! keeps the [`PairingView`] the window draws.

use std::time::Duration;

use chrono::{DateTime, TimeDelta, Utc};
use serde::Serialize;
use tokio::time::Instant;

use super::client::{ClientError, DeviceCode, PollOutcome, RelayClient};
use super::token_store::Token;

/// Seconds added to the poll interval on `slow_down` (contract 4.2).
pub const SLOW_DOWN_STEP: u64 = 5;
/// The poll interval when the server sends 0.
pub const MIN_INTERVAL: u64 = 1;

/// Why sign-in stopped without a token.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PairingFailure {
    /// The code was not approved in time (or the server no longer knows it).
    Expired,
    /// The web user chose Deny.
    Denied,
    /// Long Lines could not be reached, or answered 5xx or 429, when starting.
    Unreachable,
    /// Long Lines refused the request (a 4xx other than the pairing answers).
    Refused,
}

/// The sign-in screen's state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum PairingView {
    /// Not signing in.
    Idle,
    /// Asking Long Lines for a code.
    Starting,
    /// Showing the code and polling.
    Waiting {
        user_code: String,
        verification_uri: String,
        verification_uri_complete: String,
        expires_at: DateTime<Utc>,
    },
    /// The last attempt failed; the signed-out screen says why.
    Failed { failure: PairingFailure },
}

/// A started pairing, ready to poll.
#[derive(Clone, Debug)]
pub struct Pending {
    pub device_code: DeviceCode,
    pub user_code: String,
    pub verification_uri: String,
    pub verification_uri_complete: String,
    /// The interval in protocol seconds.
    pub interval: u64,
    /// When polling stops.
    pub deadline: Instant,
    /// The same moment on the wall clock, for the countdown.
    pub expires_at: DateTime<Utc>,
}

impl Pending {
    pub fn view(&self) -> PairingView {
        PairingView::Waiting {
            user_code: self.user_code.clone(),
            verification_uri: self.verification_uri.clone(),
            verification_uri_complete: self.verification_uri_complete.clone(),
            expires_at: self.expires_at,
        }
    }
}

/// The device name sent with `POST /device/start`: the computer's host name, without the
/// `.local` suffix macOS adds, cut to the contract's 64 characters. Never empty.
pub fn device_name_from(host_name: &str) -> String {
    let name = host_name.trim();
    let name = name
        .strip_suffix(".local")
        .or_else(|| name.strip_suffix(".localdomain"))
        .unwrap_or(name)
        .trim();
    let name: String = name
        .chars()
        .filter(|c| !c.is_control())
        .take(super::client::MAX_DEVICE_NAME)
        .collect();
    if name.is_empty() {
        FALLBACK_DEVICE_NAME.to_string()
    } else {
        name
    }
}

/// The device name when the host name is empty. Shown on the approval page.
pub const FALLBACK_DEVICE_NAME: &str = "Computer";

/// True when `url` may be opened in the user's browser: https, or http to this computer for
/// local development. The URL comes from the server, so anything else is refused.
pub fn browser_url_ok(url: &str) -> bool {
    let Some((scheme, rest)) = url.split_once("://") else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if authority.is_empty()
        || authority.contains('@')
        || url.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return false;
    }
    match scheme {
        "https" => true,
        "http" => {
            let host = match authority.rsplit_once(':') {
                Some((host, port)) if port.bytes().all(|b| b.is_ascii_digit()) => host,
                _ => authority,
            };
            matches!(host, "127.0.0.1" | "localhost" | "[::1]")
        }
        _ => false,
    }
}

fn failure_for(error: &ClientError) -> PairingFailure {
    match error {
        ClientError::Retry { .. } | ClientError::RateLimited { .. } => PairingFailure::Unreachable,
        ClientError::SignedOut | ClientError::Rejected { .. } => PairingFailure::Refused,
    }
}

/// `POST /device/start`. `second` is the length of one protocol second (tests shorten it).
pub async fn start(
    client: &RelayClient,
    device_name: &str,
    second: Duration,
) -> Result<Pending, PairingFailure> {
    let started = client.start_pairing(device_name).await.map_err(|e| {
        tracing::warn!(error = %e, "couldn't start signing in");
        failure_for(&e)
    })?;
    let lifetime = second.saturating_mul(u32::try_from(started.expires_in).unwrap_or(u32::MAX));
    Ok(Pending {
        device_code: started.device_code,
        user_code: started.user_code,
        verification_uri: started.verification_uri,
        verification_uri_complete: started.verification_uri_complete,
        interval: started.interval.max(MIN_INTERVAL),
        deadline: Instant::now() + lifetime,
        expires_at: Utc::now() + TimeDelta::from_std(lifetime).unwrap_or(TimeDelta::minutes(10)),
    })
}

/// Polls `POST /device/token` until it answers for good or the code expires. Network trouble
/// and 5xx keep polling until the deadline; a 429 waits its `Retry-After`.
pub async fn wait_for_token(
    client: &RelayClient,
    pending: &Pending,
    second: Duration,
) -> Result<Token, PairingFailure> {
    let mut interval = pending.interval;
    let mut extra_wait = Duration::ZERO;
    loop {
        let wait = second.saturating_mul(u32::try_from(interval).unwrap_or(u32::MAX)) + extra_wait;
        extra_wait = Duration::ZERO;
        if Instant::now() + wait >= pending.deadline {
            tokio::time::sleep_until(pending.deadline).await;
            return Err(PairingFailure::Expired);
        }
        tokio::time::sleep(wait).await;
        match client.poll_token(&pending.device_code).await {
            Ok(PollOutcome::Pending) => {}
            Ok(PollOutcome::SlowDown) => interval += SLOW_DOWN_STEP,
            Ok(PollOutcome::Denied) => return Err(PairingFailure::Denied),
            Ok(PollOutcome::Expired) => return Err(PairingFailure::Expired),
            Ok(PollOutcome::Approved { token, .. }) => return Ok(token),
            Err(ClientError::Retry { reason }) => {
                tracing::info!(%reason, "couldn't poll for the token; trying again");
            }
            Err(ClientError::RateLimited { retry_after }) => extra_wait = retry_after,
            Err(e) => {
                tracing::warn!(error = %e, "Long Lines refused the token poll");
                return Err(failure_for(&e));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn views_serialize_for_the_window() {
        let json = serde_json::to_value(PairingView::Failed {
            failure: PairingFailure::Expired,
        })
        .unwrap();
        assert_eq!(
            json,
            serde_json::json!({"state": "failed", "failure": "expired"})
        );
        assert_eq!(
            serde_json::to_value(PairingView::Idle).unwrap(),
            serde_json::json!({"state": "idle"})
        );
    }

    #[test]
    fn device_names() {
        assert_eq!(device_name_from("SHACK-PC"), "SHACK-PC");
        assert_eq!(
            device_name_from(" Brads-MacBook-Pro.local\n"),
            "Brads-MacBook-Pro"
        );
        assert_eq!(device_name_from("shack.localdomain"), "shack");
        assert_eq!(device_name_from(""), FALLBACK_DEVICE_NAME);
        assert_eq!(device_name_from(".local"), FALLBACK_DEVICE_NAME);
        assert_eq!(device_name_from(&"x".repeat(100)).chars().count(), 64);
    }

    #[test]
    fn only_safe_urls_open_in_the_browser() {
        assert!(browser_url_ok(
            "https://longlines.io/relay/approve?code=KQ7M-X4PD"
        ));
        assert!(browser_url_ok("http://127.0.0.1:5173/relay/approve?code=A"));
        assert!(browser_url_ok("http://localhost:5173/relay"));
        assert!(!browser_url_ok("http://longlines.io/relay"));
        assert!(!browser_url_ok("file:///etc/passwd"));
        assert!(!browser_url_ok("javascript:alert(1)"));
        assert!(!browser_url_ok("https://"));
        assert!(!browser_url_ok("https://user@evil.example/"));
        assert!(!browser_url_ok("http://localhost@evil.example/"));
        assert!(!browser_url_ok("https://longlines.io/a b"));
        assert!(!browser_url_ok("ms-settings:"));
        assert!(browser_url_ok("http://[::1]:54321/relay"));
        assert!(!browser_url_ok("http://[::2]/relay"));
    }

    #[test]
    fn client_errors_map_to_failures() {
        assert_eq!(
            failure_for(&ClientError::Retry {
                reason: "down".into()
            }),
            PairingFailure::Unreachable
        );
        assert_eq!(
            failure_for(&ClientError::RateLimited {
                retry_after: Duration::from_secs(1)
            }),
            PairingFailure::Unreachable
        );
        assert_eq!(
            failure_for(&ClientError::Rejected {
                status: 400,
                code: None
            }),
            PairingFailure::Refused
        );
        assert_eq!(
            failure_for(&ClientError::SignedOut),
            PairingFailure::Refused
        );
    }
}
