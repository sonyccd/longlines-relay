//! The Long Lines Relay API v2 client (`docs/contract.md`). The spots stream has its own client,
//! [`crate::downstream::stream`], because it has no total request timeout.
//!
//! Every response is classified the way the contract's section 8 says to act on it: 401 signs
//! out, 429 waits `Retry-After`, any other 4xx is final, 5xx and network trouble retry. The token
//! goes only into the `Authorization` header and is never logged.

use std::time::Duration;

use reqwest::header::{HeaderMap, RETRY_AFTER};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};

use super::token_store::Token;
use super::WireItem;
use crate::state::Account;

/// `GET /me` (contract 5). The v2 fields are optional so an answer without them still reads; the
/// spots stream then stays closed.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Me {
    pub callsign: String,
    pub email: String,
    #[serde(default)]
    pub relay_id: Option<String>,
    #[serde(default)]
    pub device_name: Option<String>,
    #[serde(default)]
    pub scopes: Option<Vec<String>>,
    #[serde(default)]
    pub spots_url: Option<String>,
}

impl Me {
    /// The account the window shows.
    pub fn account(&self) -> Account {
        Account {
            callsign: self.callsign.clone(),
            email: self.email.clone(),
        }
    }
}

/// The production base URL.
pub const DEFAULT_BASE_URL: &str = "https://api.longlines.io/relay/v2";
/// Overrides the base URL for development (for example the local Supabase stack).
pub const BASE_URL_ENV: &str = "LONGLINES_RELAY_API_BASE";
/// The wait after a 429 whose `Retry-After` cannot be read.
pub const DEFAULT_RETRY_AFTER: Duration = Duration::from_secs(60);
/// The longest one request may take.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// `client_name` for `POST /device/start`.
pub const CLIENT_NAME: &str = "Long Lines Relay";
/// The longest `device_name` the server accepts.
pub const MAX_DEVICE_NAME: usize = 64;
/// Posting logged contacts (contract 3.1).
pub const CONTACTS_WRITE: &str = "contacts:write";
/// Receiving the spots stream (contract 3.1).
pub const SPOTS_READ: &str = "spots:read";
/// What a v2 pairing asks for (contract 4.1).
pub const SCOPES: [&str; 2] = [CONTACTS_WRITE, SPOTS_READ];

/// The base URL from [`BASE_URL_ENV`], else production.
pub fn base_url_from_env() -> String {
    std::env::var(BASE_URL_ENV)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string())
}

/// `windows`, `macos` or `linux`, as the `User-Agent` names the OS.
pub fn os_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

/// `LongLinesRelay/<version> (<os>)`.
pub fn user_agent(relay_version: &str) -> String {
    format!("LongLinesRelay/{relay_version} ({})", os_name())
}

/// How to reach the server.
#[derive(Clone, Debug)]
pub struct ClientConfig {
    pub base_url: String,
    pub relay_version: String,
    pub timeout: Duration,
}

impl ClientConfig {
    pub fn new(base_url: impl Into<String>, relay_version: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            relay_version: relay_version.into(),
            timeout: REQUEST_TIMEOUT,
        }
    }
}

/// How a failed request should be handled.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ClientError {
    /// 401: delete the token and show the signed-out screen.
    #[error("the token was refused")]
    SignedOut,
    /// 429: send the same request again after `retry_after`.
    #[error("rate limited; retry after {retry_after:?}")]
    RateLimited { retry_after: Duration },
    /// Any other 4xx: show it and do not retry.
    #[error("refused with HTTP {status}")]
    Rejected { status: u16, code: Option<String> },
    /// 5xx, network trouble or an unreadable answer: retry with backoff.
    #[error("{reason}")]
    Retry { reason: String },
}

/// The device code from `POST /device/start`. Never shown or logged.
#[derive(Clone, PartialEq, Eq)]
pub struct DeviceCode(String);

impl DeviceCode {
    pub fn new(code: impl Into<String>) -> Self {
        Self(code.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for DeviceCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DeviceCode(<redacted>)")
    }
}

/// A started pairing (contract 4.1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PairingStart {
    pub device_code: DeviceCode,
    pub user_code: String,
    pub verification_uri: String,
    pub verification_uri_complete: String,
    pub expires_in: u64,
    pub interval: u64,
}

/// One poll of `POST /device/token` (contract 4.2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PollOutcome {
    Pending,
    /// Add 5 seconds to the interval.
    SlowDown,
    Denied,
    Expired,
    Approved {
        token: Token,
        scopes: Vec<String>,
    },
}

/// Why the server rejected one item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RejectReason {
    TooOld,
    ClockSkew,
    MycallMismatch,
    Invalid,
    /// A status or reason this version does not know: treated as rejected, never retried.
    Unrecognized(String),
}

/// One item's final answer (contract 6.4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ItemStatus {
    Created,
    Updated,
    Unchanged,
    Retracted,
    Unknown,
    Rejected(RejectReason),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemResult {
    pub external_id: String,
    pub status: ItemStatus,
}

#[derive(Deserialize)]
struct RawStart {
    device_code: String,
    user_code: String,
    verification_uri: String,
    verification_uri_complete: String,
    expires_in: u64,
    interval: u64,
}

#[derive(Deserialize)]
struct RawToken {
    access_token: String,
    #[serde(default)]
    scopes: Vec<String>,
}

#[derive(Deserialize)]
struct RawError {
    error: Option<String>,
}

#[derive(Deserialize)]
struct RawResults {
    results: Vec<RawResult>,
}

#[derive(Deserialize)]
struct RawResult {
    external_id: String,
    status: String,
    reason: Option<String>,
}

#[derive(Serialize)]
struct StartBody<'a> {
    client_name: &'a str,
    client_version: &'a str,
    device_name: &'a str,
    scopes: &'a [&'a str],
    platform: &'a str,
}

#[derive(Serialize)]
struct PollBody<'a> {
    device_code: &'a str,
}

#[derive(Serialize)]
struct ContactsBody<'a> {
    relay_version: &'a str,
    contacts: &'a [WireItem],
}

/// The HTTP client. Cheap to clone.
#[derive(Clone, Debug)]
pub struct RelayClient {
    http: reqwest::Client,
    base_url: String,
    relay_version: String,
}

impl RelayClient {
    pub fn new(config: ClientConfig) -> Result<Self, ClientError> {
        let http = reqwest::Client::builder()
            .user_agent(user_agent(&config.relay_version))
            .timeout(config.timeout)
            .connect_timeout(config.timeout.min(Duration::from_secs(10)))
            .build()
            .map_err(|e| ClientError::Retry {
                reason: format!("couldn't start the HTTP client: {e}"),
            })?;
        Ok(Self {
            http,
            base_url: config.base_url.trim_end_matches('/').to_string(),
            relay_version: config.relay_version,
        })
    }

    pub fn relay_version(&self) -> &str {
        &self.relay_version
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base_url)
    }

    /// `POST /device/start`, asking for both scopes. The device name is cut to 64 characters.
    pub async fn start_pairing(&self, device_name: &str) -> Result<PairingStart, ClientError> {
        let device_name: String = device_name.chars().take(MAX_DEVICE_NAME).collect();
        let body = StartBody {
            client_name: CLIENT_NAME,
            client_version: &self.relay_version,
            device_name: &device_name,
            scopes: &SCOPES,
            platform: os_name(),
        };
        let response = self
            .send(self.http.post(self.url("/device/start")).json(&body))
            .await?;
        let raw: RawStart = success_json(response).await?;
        Ok(PairingStart {
            device_code: DeviceCode(raw.device_code),
            user_code: raw.user_code,
            verification_uri: raw.verification_uri,
            verification_uri_complete: raw.verification_uri_complete,
            expires_in: raw.expires_in,
            interval: raw.interval,
        })
    }

    /// `POST /device/token`, once.
    pub async fn poll_token(&self, device_code: &DeviceCode) -> Result<PollOutcome, ClientError> {
        let body = PollBody {
            device_code: device_code.expose(),
        };
        let response = self
            .send(self.http.post(self.url("/device/token")).json(&body))
            .await?;
        if response.status() == StatusCode::BAD_REQUEST {
            let code = error_code(response).await;
            return match code.as_deref() {
                Some("authorization_pending") => Ok(PollOutcome::Pending),
                Some("slow_down") => Ok(PollOutcome::SlowDown),
                Some("access_denied") => Ok(PollOutcome::Denied),
                Some("expired_token") => Ok(PollOutcome::Expired),
                _ => Err(ClientError::Rejected { status: 400, code }),
            };
        }
        let raw: RawToken = success_json(response).await?;
        Ok(PollOutcome::Approved {
            token: Token::new(raw.access_token),
            scopes: raw.scopes,
        })
    }

    /// `GET /me`.
    pub async fn me(&self, token: &Token) -> Result<Me, ClientError> {
        let request = self.http.get(self.url("/me")).bearer_auth(token.expose());
        success_json(self.send(request).await?).await
    }

    /// `POST /contacts` with 1 to 100 items. The results are in item order and as many as the
    /// items, or the whole call is a [`ClientError::Retry`] (a resend is safe).
    pub async fn submit(
        &self,
        token: &Token,
        items: &[WireItem],
    ) -> Result<Vec<ItemResult>, ClientError> {
        let body = ContactsBody {
            relay_version: &self.relay_version,
            contacts: items,
        };
        let request = self
            .http
            .post(self.url("/contacts"))
            .bearer_auth(token.expose())
            .json(&body);
        let raw: RawResults = success_json(self.send(request).await?).await?;
        if raw.results.len() != items.len() {
            return Err(ClientError::Retry {
                reason: "Long Lines answered with the wrong number of results".into(),
            });
        }
        Ok(raw.results.into_iter().map(item_result).collect())
    }

    /// Sends a request and turns every non-success answer except a 400 into a [`ClientError`].
    /// A 400 is passed through for `poll_token`; [`success_json`] rejects it elsewhere.
    async fn send(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<reqwest::Response, ClientError> {
        let response = request.send().await.map_err(|e| {
            tracing::debug!(error = %e.without_url(), "request to Long Lines failed");
            ClientError::Retry {
                reason: "couldn't reach Long Lines".into(),
            }
        })?;
        let status = response.status();
        tracing::debug!(
            status = status.as_u16(),
            path = response.url().path(),
            "Long Lines answered"
        );
        if status.is_success() || status == StatusCode::BAD_REQUEST {
            return Ok(response);
        }
        let wait = retry_after(response.headers());
        Err(classify(status, wait, response).await)
    }
}

/// Section 8's table for a non-success status other than 400.
async fn classify(
    status: StatusCode,
    retry_after: Duration,
    response: reqwest::Response,
) -> ClientError {
    match status.as_u16() {
        401 => ClientError::SignedOut,
        429 => ClientError::RateLimited { retry_after },
        s if status.is_server_error() => ClientError::Retry {
            reason: format!("Long Lines answered with error {s}"),
        },
        s if status.is_client_error() => ClientError::Rejected {
            status: s,
            code: error_code(response).await,
        },
        s => ClientError::Retry {
            reason: format!("Long Lines answered with HTTP {s}"),
        },
    }
}

/// `Retry-After` in whole seconds; anything else waits [`DEFAULT_RETRY_AFTER`].
pub fn retry_after(headers: &HeaderMap) -> Duration {
    headers
        .get(RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<u64>().ok())
        .map_or(DEFAULT_RETRY_AFTER, Duration::from_secs)
}

async fn error_code(response: reqwest::Response) -> Option<String> {
    response.json::<RawError>().await.ok().and_then(|e| e.error)
}

async fn success_json<T: serde::de::DeserializeOwned>(
    response: reqwest::Response,
) -> Result<T, ClientError> {
    if response.status() == StatusCode::BAD_REQUEST {
        return Err(ClientError::Rejected {
            status: 400,
            code: error_code(response).await,
        });
    }
    response.json::<T>().await.map_err(|e| {
        tracing::debug!(error = %e.without_url(), "unreadable answer from Long Lines");
        ClientError::Retry {
            reason: "Long Lines gave an unreadable answer".into(),
        }
    })
}

fn item_result(raw: RawResult) -> ItemResult {
    let status = match (raw.status.as_str(), raw.reason.as_deref()) {
        ("created", _) => ItemStatus::Created,
        ("updated", _) => ItemStatus::Updated,
        ("unchanged", _) => ItemStatus::Unchanged,
        ("retracted", _) => ItemStatus::Retracted,
        ("unknown", _) => ItemStatus::Unknown,
        ("rejected", Some("too_old")) => ItemStatus::Rejected(RejectReason::TooOld),
        ("rejected", Some("clock_skew")) => ItemStatus::Rejected(RejectReason::ClockSkew),
        ("rejected", Some("mycall_mismatch")) => ItemStatus::Rejected(RejectReason::MycallMismatch),
        ("rejected", Some("invalid")) => ItemStatus::Rejected(RejectReason::Invalid),
        ("rejected", reason) => {
            ItemStatus::Rejected(RejectReason::Unrecognized(reason.unwrap_or("").to_string()))
        }
        (other, _) => ItemStatus::Rejected(RejectReason::Unrecognized(other.to_string())),
    };
    ItemResult {
        external_id: raw.external_id,
        status,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::HeaderValue;

    #[test]
    fn retry_after_reads_seconds_or_waits_a_minute() {
        let mut headers = HeaderMap::new();
        assert_eq!(retry_after(&headers), DEFAULT_RETRY_AFTER);
        headers.insert(RETRY_AFTER, HeaderValue::from_static("7"));
        assert_eq!(retry_after(&headers), Duration::from_secs(7));
        headers.insert(
            RETRY_AFTER,
            HeaderValue::from_static("Wed, 21 Oct 2026 07:28:00 GMT"),
        );
        assert_eq!(retry_after(&headers), DEFAULT_RETRY_AFTER);
        headers.insert(RETRY_AFTER, HeaderValue::from_static("-3"));
        assert_eq!(retry_after(&headers), DEFAULT_RETRY_AFTER);
    }

    #[test]
    fn unknown_statuses_and_reasons_are_rejections() {
        let raw = |status: &str, reason: Option<&str>| RawResult {
            external_id: "x".into(),
            status: status.into(),
            reason: reason.map(String::from),
        };
        assert_eq!(
            item_result(raw("created", None)).status,
            ItemStatus::Created
        );
        assert_eq!(
            item_result(raw("rejected", Some("mycall_mismatch"))).status,
            ItemStatus::Rejected(RejectReason::MycallMismatch)
        );
        assert_eq!(
            item_result(raw("rejected", Some("banana"))).status,
            ItemStatus::Rejected(RejectReason::Unrecognized("banana".into()))
        );
        assert_eq!(
            item_result(raw("queued", None)).status,
            ItemStatus::Rejected(RejectReason::Unrecognized("queued".into()))
        );
    }

    #[test]
    fn user_agent_names_the_os() {
        let ua = user_agent("0.1.0");
        assert!(ua.starts_with("LongLinesRelay/0.1.0 ("));
        assert!(["(windows)", "(macos)", "(linux)"]
            .iter()
            .any(|os| ua.ends_with(os)));
    }

    #[test]
    fn device_code_is_redacted() {
        assert!(!format!("{:?}", DeviceCode::new("secret-code")).contains("secret"));
    }
}
