//! RaceRoom portal registration and announcements.
//!
//! The server browser is the only join path that works (S-7 failed), so every
//! manager registers a slave with the portal, creates its settings there, and
//! announces the running server. Registration is per (public IP, secret); a
//! fresh secret per container gives an independent quota.

use std::{sync::Arc, time::Duration};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tokio::sync::RwLock;

use crate::{
    config::Config,
    crypto::{MW_SECURE_KEY, decrypt_message, encrypt_message},
    settings::{field, i64_field, string_field},
};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct Registration {
    pub slave_id: i64,
    pub slave_ip: String,
    pub dedi_quota: i64,
    pub ad_packs: Option<Vec<String>>,
    pub event_packs: Option<Value>,
    pub announce_endpoint: Option<String>,
    pub announce_interval: Option<String>,
    pub announce_player_coordinates_endpoint: Option<String>,
    pub announce_player_coordinates_interval: Option<String>,
    pub get_settings_endpoint: Option<String>,
    pub set_settings_endpoint: Option<String>,
    pub get_port_endpoint: Option<String>,
    pub delete_settings_endpoint: Option<String>,
    pub web_portal_race_results_endpoint: Option<String>,
    pub grid_request_endpoint: Option<String>,
    pub game_content_endpoint: Option<String>,
    pub report_slave_error_endpoint: Option<String>,
    pub can_have_ranked_settings: bool,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Registration {
    pub fn quota(&self) -> usize {
        usize::try_from(self.dedi_quota).unwrap_or(0)
    }

    pub fn announce_interval(&self) -> Duration {
        self.announce_interval
            .as_deref()
            .and_then(parse_timespan)
            .unwrap_or(Duration::from_secs(10))
    }

    pub fn announce_players_interval(&self) -> Duration {
        self.announce_player_coordinates_interval
            .as_deref()
            .and_then(parse_timespan)
            .unwrap_or(Duration::from_secs(2))
    }

    pub fn ad_packs(&self) -> &[String] {
        self.ad_packs.as_deref().unwrap_or_default()
    }
}

#[derive(Clone)]
pub struct Portal {
    config: Arc<Config>,
    http: reqwest::Client,
    state: Arc<RwLock<PortalState>>,
}

#[derive(Clone, Debug, Default)]
struct PortalState {
    secret: Option<String>,
    registration: Option<Registration>,
}

#[derive(Debug, thiserror::Error)]
pub enum PortalError {
    #[error("not registered with the RaceRoom portal")]
    NotRegistered,
    #[error("{message} (code {code})")]
    Api {
        code: i64,
        message: String,
        not_serving: bool,
    },
    #[error("HTTP {status}: {body}")]
    Http { status: String, body: String },
    #[error("portal response is invalid: {0}")]
    InvalidResponse(String),
    #[error(transparent)]
    Crypto(#[from] anyhow::Error),
    #[error(transparent)]
    Request(#[from] reqwest::Error),
}

fn portal_api_error(value: &Value) -> Option<PortalError> {
    if let Some(errors) = field(value, &["errors", "Errors"]) {
        let status = i64_field(value, &["status", "Status"], 400);
        let title = string_field(
            value,
            &["title", "Title"],
            "RaceRoom portal rejected the request",
        );
        return Some(PortalError::Api {
            code: status,
            message: format!("{title}: {errors}"),
            not_serving: matches!(status, 403 | 254),
        });
    }
    let error = field(
        value,
        &["MultiPlayerErrorResponse", "multiPlayerErrorResponse"],
    )
    .or_else(|| field(value, &["ErrorResponse", "errorResponse"]))?;
    let code = i64_field(error, &["ErrorCode", "errorCode"], 0);
    let message = string_field(
        error,
        &["Message", "message"],
        "RaceRoom portal rejected the request",
    );
    Some(PortalError::Api {
        code,
        message,
        not_serving: matches!(code, 403 | 254),
    })
}

impl Portal {
    pub fn new(config: Arc<Config>, http: reqwest::Client) -> Self {
        Self {
            config,
            http,
            state: Arc::new(RwLock::new(PortalState::default())),
        }
    }

    pub async fn secret(&self) -> Option<String> {
        self.state.read().await.secret.clone()
    }

    pub async fn registration(&self) -> Option<Registration> {
        let state = self.state.read().await;
        if !state
            .secret
            .as_deref()
            .is_some_and(|secret| !secret.trim().is_empty())
        {
            return None;
        }
        state.registration.clone()
    }

    pub async fn is_registered(&self) -> bool {
        self.registration()
            .await
            .is_some_and(|registration| registration.slave_id > 0)
    }

    pub async fn register(&self, secret: &str) -> Result<Registration, PortalError> {
        let secret = secret.trim();
        if secret.is_empty() {
            return Err(PortalError::Api {
                code: 250,
                message: "Secret cannot be empty!".to_string(),
                not_serving: false,
            });
        }
        let response = self
            .http
            .post(&self.config.portal_registration_url)
            .header("secret", secret)
            .header("MW-Encrypted", "1")
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(Vec::new())
            .send()
            .await?;
        let status = response.status();
        let text = response.text().await?;
        let value = decode_response(&text).map_err(PortalError::InvalidResponse)?;
        if let Some(error) = portal_api_error(&value) {
            return Err(error);
        }
        if !status.is_success() {
            return Err(PortalError::Http {
                status: status.to_string(),
                body: text,
            });
        }
        let registration: Registration = serde_json::from_value(value).map_err(|error| {
            PortalError::InvalidResponse(format!("registration payload: {error}"))
        })?;
        if registration.slave_id <= 0 {
            return Err(PortalError::InvalidResponse(
                "registration response has no SlaveId".to_string(),
            ));
        }
        let mut state = self.state.write().await;
        state.secret = Some(secret.to_string());
        state.registration = Some(registration.clone());
        tracing::info!(
            slave_id = registration.slave_id,
            slave_ip = %registration.slave_ip,
            quota = registration.dedi_quota,
            "registered with the RaceRoom portal"
        );
        Ok(registration)
    }

    pub async fn verify(&self) -> Result<Registration, PortalError> {
        let secret = self.secret().await.ok_or(PortalError::NotRegistered)?;
        self.register(&secret).await
    }

    async fn call(
        &self,
        method: reqwest::Method,
        url: &str,
        body: Option<&Value>,
        timeout: Option<Duration>,
    ) -> Result<Value, PortalError> {
        let secret = self.secret().await.ok_or(PortalError::NotRegistered)?;
        let mut request = self
            .http
            .request(method.clone(), url)
            .header("secret", secret)
            .header("MW-Encrypted", "1")
            .header(reqwest::header::CONTENT_TYPE, "application/json");
        if let Some(body) = body {
            let text = serde_json::to_string(body)
                .map_err(|error| PortalError::InvalidResponse(error.to_string()))?;
            let encrypted = encrypt_message(&text, MW_SECURE_KEY)?;
            request = request.body(encrypted);
        } else if method == reqwest::Method::POST {
            request = request.body(Vec::new());
        }
        if let Some(timeout) = timeout {
            request = request.timeout(timeout);
        }
        let response = request.send().await?;
        let status = response.status();
        let text = response.text().await?;
        let value = decode_response(&text).map_err(PortalError::InvalidResponse)?;
        if let Some(error) = portal_api_error(&value) {
            return Err(error);
        }
        if !status.is_success() {
            return Err(PortalError::Http {
                status: status.to_string(),
                body: text,
            });
        }
        Ok(value)
    }

    pub async fn set_settings(&self, settings: &Value) -> Result<i64, PortalError> {
        let registration = self
            .registration()
            .await
            .ok_or(PortalError::NotRegistered)?;
        let url = registration
            .set_settings_endpoint
            .context("registration has no SetSettingsEndpoint")
            .map_err(PortalError::Crypto)?;
        let value = self
            .call(reqwest::Method::POST, &url, Some(settings), None)
            .await?;
        id_from_value(&value, &["ProcessId", "processId", "Id", "id"])
            .ok_or_else(|| PortalError::InvalidResponse(format!("set settings returned {value}")))
    }

    pub async fn get_port(&self) -> Result<i64, PortalError> {
        let registration = self
            .registration()
            .await
            .ok_or(PortalError::NotRegistered)?;
        let url = registration
            .get_port_endpoint
            .context("registration has no GetPortEndpoint")
            .map_err(PortalError::Crypto)?;
        let value = self.call(reqwest::Method::GET, &url, None, None).await?;
        id_from_value(&value, &["Port", "port"])
            .ok_or_else(|| PortalError::InvalidResponse(format!("get port returned {value}")))
    }

    pub async fn post_grid(&self, request: &Value) -> Result<Value, PortalError> {
        let registration = self
            .registration()
            .await
            .ok_or(PortalError::NotRegistered)?;
        let url = registration
            .grid_request_endpoint
            .context("registration has no GridRequestEndpoint")
            .map_err(PortalError::Crypto)?;
        self.call(reqwest::Method::POST, &url, Some(request), None)
            .await
    }

    pub async fn post_result(&self, payload: &Value) -> Result<Value, PortalError> {
        let registration = self
            .registration()
            .await
            .ok_or(PortalError::NotRegistered)?;
        let url = registration
            .web_portal_race_results_endpoint
            .context("registration has no WebPortalRaceResultsEndpoint")
            .map_err(PortalError::Crypto)?;
        let mut last_error = None;
        for attempt in 0..3 {
            match self
                .call(reqwest::Method::POST, &url, Some(payload), None)
                .await
            {
                Ok(value) => return Ok(value),
                Err(error) => {
                    tracing::warn!(attempt = attempt + 1, %error, "portal result post failed");
                    last_error = Some(error);
                    tokio::time::sleep(Duration::from_secs(1 << attempt)).await;
                }
            }
        }
        Err(last_error.unwrap_or(PortalError::NotRegistered))
    }

    pub async fn announce_server(&self, payload: &Value) -> Result<bool, PortalError> {
        let registration = self
            .registration()
            .await
            .ok_or(PortalError::NotRegistered)?;
        let timeout = registration.announce_interval() * 2;
        let url = registration
            .announce_endpoint
            .clone()
            .context("registration has no AnnounceEndpoint")
            .map_err(PortalError::Crypto)?;
        let value = self
            .call(reqwest::Method::POST, &url, Some(payload), Some(timeout))
            .await?;
        if value.as_bool().is_none() {
            tracing::warn!(response = %value, "unexpected announcement response");
        }
        Ok(value.as_bool().unwrap_or(true))
    }

    pub async fn announce_players(&self, payload: &Value) -> Result<bool, PortalError> {
        let registration = self
            .registration()
            .await
            .ok_or(PortalError::NotRegistered)?;
        let timeout = registration.announce_players_interval() * 2;
        let url = registration
            .announce_player_coordinates_endpoint
            .clone()
            .context("registration has no AnnouncePlayerCoordinatesEndpoint")
            .map_err(PortalError::Crypto)?;
        let value = self
            .call(reqwest::Method::POST, &url, Some(payload), Some(timeout))
            .await?;
        Ok(value.as_bool().unwrap_or(true))
    }
}

fn id_from_value(value: &Value, names: &[&str]) -> Option<i64> {
    match value {
        Value::Number(number) => number.as_i64(),
        Value::String(text) => text.parse().ok(),
        Value::Object(_) => {
            let id = i64_field(value, names, 0);
            (id != 0).then_some(id)
        }
        _ => None,
    }
}

fn decode_response(text: &str) -> Result<Value, String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(Value::Null);
    }
    let plain = if text.contains("EncryptedMessage") {
        match decrypt_message(text, MW_SECURE_KEY) {
            Ok(bytes) => String::from_utf8(bytes).map_err(|error| error.to_string())?,
            Err(_) => text.to_string(),
        }
    } else {
        text.to_string()
    };
    serde_json::from_str(&plain).map_err(|error| format!("{error}: {plain}"))
}

pub fn parse_timespan(value: &str) -> Option<Duration> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    let negative = value.starts_with('-');
    let value = value.trim_start_matches('-');
    let mut parts = value.split(':');
    let hours: f64 = parts.next()?.parse().ok()?;
    let minutes: f64 = parts.next()?.parse().ok()?;
    let seconds: f64 = parts.next()?.parse().ok()?;
    let total = hours * 3600.0 + minutes * 60.0 + seconds;
    if !total.is_finite() || total < 0.0 {
        return None;
    }
    let duration = Duration::from_secs_f64(total);
    if negative {
        Some(Duration::ZERO)
    } else {
        Some(duration)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_dotnet_timespans() {
        assert_eq!(parse_timespan("00:00:10"), Some(Duration::from_secs(10)));
        assert_eq!(parse_timespan("00:00:02"), Some(Duration::from_secs(2)));
        assert_eq!(
            parse_timespan("01:02:03.5000000"),
            Some(Duration::from_secs_f64(3723.5))
        );
        assert_eq!(parse_timespan("bogus"), None);
    }

    #[test]
    fn reads_registration_shape() {
        let value = json!({
            "SlaveId": 7,
            "SlaveIp": "203.0.113.5",
            "DediQuota": 5,
            "AdPacks": null,
            "EventPacks": null,
            "AnnounceEndpoint": "https://example/Announce",
            "AnnounceInterval": "00:00:10",
            "AnnouncePlayerCoordinatesEndpoint": "https://example/Players",
            "AnnouncePlayerCoordinatesInterval": "00:00:02",
            "GetSettingsEndpoint": "https://example/Settings",
            "SetSettingsEndpoint": "https://example/SetSettings",
            "GetPortEndpoint": "https://example/Port",
            "DeleteSettingsEndpoint": "https://example/Delete",
            "WebPortalRaceResultsEndpoint": "https://example/Result",
            "GridRequestEndpoint": "https://example/Grid",
            "GameContentEndpoint": "https://example/Content",
            "ReportSlaveErrorEndpoint": "https://example/Error",
            "CanHaveRankedSettings": true
        });
        let registration: Registration = serde_json::from_value(value).unwrap();
        assert_eq!(registration.slave_id, 7);
        assert_eq!(registration.quota(), 5);
        assert_eq!(registration.announce_interval(), Duration::from_secs(10));
        assert!(registration.can_have_ranked_settings);
        assert!(registration.ad_packs().is_empty());
    }

    #[tokio::test]
    async fn registers_and_allocates_through_the_encrypted_envelope() {
        use axum::{
            Router,
            body::Bytes,
            http::HeaderMap,
            routing::{get, post},
        };
        use std::path::PathBuf;

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let base = format!("http://{address}");
        let registration = encrypt_message(
            &json!({
                "SlaveId": 7,
                "SlaveIp": "203.0.113.5",
                "DediQuota": 5,
                "AnnounceEndpoint": format!("{base}/announce"),
                "AnnounceInterval": "00:00:10",
                "AnnouncePlayerCoordinatesEndpoint": format!("{base}/players"),
                "AnnouncePlayerCoordinatesInterval": "00:00:02",
                "GetSettingsEndpoint": format!("{base}/settings"),
                "SetSettingsEndpoint": format!("{base}/settings"),
                "GetPortEndpoint": format!("{base}/port"),
                "WebPortalRaceResultsEndpoint": format!("{base}/result"),
                "GridRequestEndpoint": format!("{base}/grid"),
                "CanHaveRankedSettings": true
            })
            .to_string(),
            MW_SECURE_KEY,
        )
        .unwrap();
        let settings =
            encrypt_message(&json!({ "ProcessId": 42 }).to_string(), MW_SECURE_KEY).unwrap();
        let port = encrypt_message(&json!({ "Port": 60003 }).to_string(), MW_SECURE_KEY).unwrap();

        let app = Router::new()
            .route(
                "/register",
                post({
                    let registration = registration.clone();
                    move |headers: HeaderMap| {
                        let registration = registration.clone();
                        async move {
                            assert_eq!(headers["secret"], "test-secret");
                            registration
                        }
                    }
                }),
            )
            .route(
                "/settings",
                post({
                    let settings = settings.clone();
                    move |body: Bytes| {
                        let settings = settings.clone();
                        async move {
                            assert!(!body.is_empty());
                            settings
                        }
                    }
                }),
            )
            .route(
                "/port",
                get({
                    let port = port.clone();
                    move || {
                        let port = port.clone();
                        async move { port }
                    }
                }),
            );
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        let config = Config {
            bind: "127.0.0.1:0".parse().unwrap(),
            data_dir: PathBuf::from("/tmp/metarace-portal-test"),
            results_dir: PathBuf::from("/tmp/metarace-portal-test/results"),
            settings: json!({}),
            game_port: 60000,
            game_port_override: None,
            dedi_ip: None,
            public_url: None,
            game_dir: None,
            game_zip: None,
            portal_secret: Some("test-secret".to_string()),
            portal_registration_url: format!("{base}/register"),
            portal_disabled: false,
            game_launcher: "/bin/true".to_string(),
            game_launcher_prefix_args: Vec::new(),
            wine_prefix: PathBuf::from("/tmp/metarace-portal-test/prefix"),
            wineboot_bin: "wineboot".to_string(),
            wineserver_bin: "wineserver".to_string(),
            control_token: Some("token".to_string()),
            allow_insecure: false,
            result: None,
            stop_grace: Duration::from_secs(1),
            log_level: "error".to_string(),
        };
        let config = Arc::new(config);
        let portal = Portal::new(config, reqwest::Client::new());

        let registered = portal.register("test-secret").await.unwrap();
        assert_eq!(registered.slave_id, 7);
        assert_eq!(registered.slave_ip, "203.0.113.5");
        assert!(portal.is_registered().await);
        assert_eq!(
            portal
                .set_settings(&json!({ "Name": "test" }))
                .await
                .unwrap(),
            42
        );
        assert_eq!(portal.get_port().await.unwrap(), 60003);
    }
}
