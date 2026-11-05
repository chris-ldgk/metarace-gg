use axum::{
    Json, Router,
    body::{Body, Bytes},
    extract::{Path as AxumPath, Request, State},
    http::{HeaderValue, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde_json::{Value, json};
use subtle::ConstantTimeEq;
use tracing::warn;

use crate::{
    crypto::{DEDI_SECURE_KEY, decrypt_message, encrypt_message},
    error::ApiError,
    settings,
    state::AppState,
};

pub fn router(state: AppState) -> Router {
    let control = Router::new()
        .route("/state", get(get_state))
        .route("/stop", post(stop))
        .route("/results", get(list_results))
        .route("/results/{name}", get(download_result))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            require_control_token,
        ));

    Router::new()
        .route("/healthz", get(healthz))
        .route("/slave/DedicatedServerState", post(slave_state))
        .route("/slave/MultiplayerEventResult", post(slave_event_result))
        .route("/slave/GridRequest", post(slave_grid_request))
        .merge(control)
        .with_state(state)
}

async fn require_control_token(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let Some(expected) = state.config.control_token.as_deref() else {
        return next.run(request).await;
    };
    let provided = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::trim)
        .unwrap_or_default();
    if provided.as_bytes().ct_eq(expected.as_bytes()).into() {
        next.run(request).await
    } else {
        ApiError::unauthorized().into_response()
    }
}

async fn healthz() -> &'static str {
    "ok"
}

async fn get_state(State(state): State<AppState>) -> Json<Value> {
    Json(json!({
        "game": state.process.state().await,
        "portal": state.portal_status().await,
    }))
}

async fn stop(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    state.stop().await.map_err(ApiError::internal_anyhow)?;
    Ok(Json(json!({ "stopped": true })))
}

async fn list_results(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let results = state
        .list_results()
        .await
        .map_err(ApiError::internal_anyhow)?;
    Ok(Json(json!(results)))
}

async fn download_result(
    State(state): State<AppState>,
    AxumPath(name): AxumPath<String>,
) -> Result<Response, ApiError> {
    let bytes = state
        .read_result(&name)
        .await
        .map_err(|error| ApiError::not_found(error.to_string()))?;
    Ok((
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        )],
        bytes,
    )
        .into_response())
}

async fn slave_state(State(state): State<AppState>, body: Bytes) -> Response {
    let value = match decode_slave_payload(&body) {
        Ok(value) => value,
        Err(error) => return ApiError::bad_request(error).into_response(),
    };
    state.process.update_from_slave(&value).await;
    let ranked = state.ranked().await;
    encrypted_json_response(&json!({ "Ranked": ranked }))
}

async fn slave_grid_request(State(state): State<AppState>, body: Bytes) -> Response {
    if !state.portal.is_registered().await {
        return (
            axum::http::StatusCode::NOT_IMPLEMENTED,
            Json(json!({ "message": "not registered with the RaceRoom portal" })),
        )
            .into_response();
    }
    let value = match decode_slave_payload(&body) {
        Ok(value) => value,
        Err(error) => return ApiError::bad_request(error).into_response(),
    };
    match state.portal.post_grid(&value).await {
        Ok(response) => encrypted_json_response(&response),
        Err(error) => ApiError::internal(error.to_string()).into_response(),
    }
}

async fn slave_event_result(State(state): State<AppState>, body: Bytes) -> Response {
    let value = match decode_slave_payload(&body) {
        Ok(value) => value,
        Err(error) => {
            warn!(%error, "cannot decode the result callback");
            return encrypted_result_response(false);
        }
    };
    match state.handle_result(&value).await {
        Ok(path) => {
            tracing::info!(path = %path.display(), "stored the race result");
            encrypted_result_response(true)
        }
        Err(error) => {
            warn!(%error, "result delivery failed");
            encrypted_result_response(false)
        }
    }
}

fn decode_slave_payload(body: &Bytes) -> Result<Value, String> {
    let raw = String::from_utf8_lossy(body);
    if raw.trim().is_empty() {
        return Err("empty slave payload".to_string());
    }
    if raw.contains("EncryptedMessage") {
        let plain = decrypt_message(&raw, DEDI_SECURE_KEY).map_err(|error| error.to_string())?;
        let text = String::from_utf8(plain).map_err(|error| error.to_string())?;
        return serde_json::from_str(&text).map_err(|error| error.to_string());
    }
    if let Ok(value) = serde_json::from_str::<Value>(&raw) {
        if let Some(data) = settings::field(&value, &["Data", "data"]).and_then(Value::as_str)
            && data.contains("EncryptedMessage")
        {
            let plain =
                decrypt_message(data, DEDI_SECURE_KEY).map_err(|error| error.to_string())?;
            let text = String::from_utf8(plain).map_err(|error| error.to_string())?;
            return serde_json::from_str(&text).map_err(|error| error.to_string());
        }
        return Ok(value);
    }
    Err("payload is not JSON".to_string())
}

fn encrypted_result_response(success: bool) -> Response {
    encrypted_json_response(&json!({ "MultiplayerEventResultResult": success }))
}

fn encrypted_json_response(value: &Value) -> Response {
    match encrypt_message(&value.to_string(), DEDI_SECURE_KEY) {
        Ok(bytes) => (
            [(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            )],
            Body::from(bytes),
        )
            .into_response(),
        Err(error) => ApiError::internal(error.to_string()).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::{Config, ResultTarget},
        settings::default_track_layout,
    };
    use axum::{body::to_bytes, http::StatusCode};
    use std::{
        path::PathBuf,
        sync::{Arc, Mutex},
        time::Duration,
    };
    use tower::ServiceExt;

    fn test_config(dir: &std::path::Path) -> Config {
        Config {
            bind: "127.0.0.1:0".parse().unwrap(),
            data_dir: dir.to_path_buf(),
            results_dir: dir.join("results"),
            settings: json!({
                "TrackLayouts": [default_track_layout(1)],
                "RankedServer": 1
            }),
            game_port: 60000,
            game_port_override: None,
            dedi_ip: None,
            public_url: None,
            game_dir: Some(dir.join("game")),
            game_zip: None,
            portal_secret: None,
            portal_registration_url: "http://127.0.0.1:1/register".to_string(),
            portal_disabled: true,
            game_launcher: "/bin/true".to_string(),
            game_launcher_prefix_args: Vec::new(),
            wine_prefix: PathBuf::from("/tmp/unused-prefix"),
            wineboot_bin: "wineboot".to_string(),
            wineserver_bin: "wineserver".to_string(),
            control_token: Some("secret".to_string()),
            allow_insecure: false,
            result: None,
            stop_grace: Duration::from_secs(1),
            log_level: "error".to_string(),
        }
    }

    fn test_state(dir: &std::path::Path) -> AppState {
        let config = test_config(dir);
        std::fs::create_dir_all(&config.results_dir).unwrap();
        AppState::build(config).unwrap()
    }

    fn request(uri: &str) -> Request {
        Request::builder().uri(uri).body(Body::empty()).unwrap()
    }

    #[tokio::test]
    async fn health_is_public_and_control_needs_a_token() {
        let dir = tempfile::tempdir().unwrap();
        let app = router(test_state(dir.path()));

        let response = app.clone().oneshot(request("/healthz")).await.unwrap();
        assert_eq!(response.status(), 200);

        let response = app.clone().oneshot(request("/state")).await.unwrap();
        assert_eq!(response.status(), 401);

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/state")
                    .header("authorization", "Bearer wrong")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 401);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/state")
                    .header("authorization", "Bearer secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let state: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(state["game"]["port"], 60000);
        assert_eq!(state["game"]["timePort"], 60002);
        assert_eq!(state["portal"]["registered"], false);
    }

    #[tokio::test]
    async fn state_callback_reports_ranked_status() {
        let dir = tempfile::tempdir().unwrap();
        let app = router(test_state(dir.path()));
        let payload = encrypt_message(
            &json!({ "Port": 60000, "Session": 1, "Players": [] }).to_string(),
            DEDI_SECURE_KEY,
        )
        .unwrap();
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/slave/DedicatedServerState")
                    .body(Body::from(payload))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let plain = decrypt_message(core::str::from_utf8(&body).unwrap(), DEDI_SECURE_KEY).unwrap();
        let value: Value = serde_json::from_slice(&plain).unwrap();
        assert_eq!(value["Ranked"], true);
    }

    #[tokio::test]
    async fn result_callback_is_stored_and_acknowledged() {
        let dir = tempfile::tempdir().unwrap();
        let state = test_state(dir.path());
        let value = json!({ "Sessions": [{ "Players": [] }] });
        state.handle_result(&value).await.unwrap();
        state.handle_result(&value).await.unwrap();
        let entries = state.list_results().await.unwrap();
        assert_eq!(entries.len(), 1, "duplicates must reuse the stored file");
        let name = entries[0]["name"].as_str().unwrap();
        let stored: Value =
            serde_json::from_slice(&state.read_result(name).await.unwrap()).unwrap();
        assert_eq!(stored, value);
        let stored_state = state.process.state().await;
        assert!(stored_state.result_received_at.is_some());
        assert!(stored_state.result_delivered_at.is_some());
    }

    type Captured = Arc<Mutex<Vec<(http::HeaderMap, Vec<u8>)>>>;

    #[tokio::test]
    async fn pushes_the_result_with_run_headers() {
        let received: Captured = Arc::new(Mutex::new(Vec::new()));
        let sink = received.clone();
        let app = Router::new().route(
            "/ingest",
            post(move |headers: http::HeaderMap, body: Bytes| {
                let sink = sink.clone();
                async move {
                    sink.lock().unwrap().push((headers, body.to_vec()));
                    StatusCode::OK
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        let dir = tempfile::tempdir().unwrap();
        let mut config = test_config(dir.path());
        config.result = Some(ResultTarget {
            url: format!("http://{address}/ingest"),
            token: "run-token".to_string(),
            run_id: Some("run-42".to_string()),
            server_id: Some("split-7".to_string()),
            attempts: 2,
            timeout: Duration::from_secs(5),
        });
        std::fs::create_dir_all(&config.results_dir).unwrap();
        let state = AppState::build(config).unwrap();

        let value = json!({ "Sessions": [], "Race": "example" });
        let path = state.handle_result(&value).await.unwrap();
        assert!(path.exists());

        let captured = received.lock().unwrap();
        assert_eq!(captured.len(), 1);
        let (headers, body) = &captured[0];
        assert_eq!(headers["authorization"], "Bearer run-token");
        assert_eq!(headers["x-metarace-run-id"], "run-42");
        assert_eq!(headers["x-metarace-server-id"], "split-7");
        assert_eq!(headers["content-type"], "application/json");
        assert_eq!(headers["x-metarace-result-sha256"].len(), 64);
        assert_eq!(serde_json::from_slice::<Value>(body).unwrap(), value);
    }

    #[tokio::test]
    async fn a_failed_push_is_not_acknowledged() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);

        let dir = tempfile::tempdir().unwrap();
        let mut config = test_config(dir.path());
        config.result = Some(ResultTarget {
            url: format!("http://{address}/ingest"),
            token: "run-token".to_string(),
            run_id: None,
            server_id: None,
            attempts: 1,
            timeout: Duration::from_secs(1),
        });
        std::fs::create_dir_all(&config.results_dir).unwrap();
        let state = AppState::build(config).unwrap();

        assert!(state.handle_result(&json!({})).await.is_err());
        let stored_state = state.process.state().await;
        assert!(stored_state.result_received_at.is_some());
        assert!(stored_state.result_delivered_at.is_none());
        assert_eq!(state.list_results().await.unwrap().len(), 1);
    }
}
