use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::{Context, Result, bail};
use chrono::Utc;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::{
    config::Config,
    portal::{Portal, Registration},
    process::ProcessManager,
    settings,
};

#[derive(Clone)]
struct StoredResult {
    path: PathBuf,
    delivered: bool,
}

#[derive(Clone)]
struct RunContext {
    registration: Registration,
    settings: Value,
    port: i64,
}

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub http: reqwest::Client,
    pub process: ProcessManager,
    pub portal: Portal,
    results: Arc<Mutex<HashMap<String, StoredResult>>>,
    run: Arc<RwLock<Option<RunContext>>>,
}

impl AppState {
    pub fn build(config: Config) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .context("build HTTP client")?;
        let config = Arc::new(config);
        let process = ProcessManager::new(config.clone());
        let portal = Portal::new(config.clone(), http.clone());
        Ok(Self {
            config,
            http,
            process,
            portal,
            results: Arc::new(Mutex::new(HashMap::new())),
            run: Arc::new(RwLock::new(None)),
        })
    }

    /// Registers with the portal (or uses the local port), creates the settings
    /// there, then starts the one game server.
    pub async fn start(&self) -> Result<()> {
        if self.config.portal_enabled() {
            let secret = self
                .config
                .portal_secret
                .clone()
                .context("METARACE_PORTAL_SECRET is required")?;
            let registration = self
                .portal
                .register(&secret)
                .await
                .map_err(|error| anyhow::anyhow!("portal registration failed: {error}"))?;
            let mut settings = self.config.settings.clone();
            if let Some(object) = settings.as_object_mut() {
                object.insert("Id".to_string(), json!(0));
            }
            let settings_id = self
                .portal
                .set_settings(&settings)
                .await
                .map_err(|error| anyhow::anyhow!("portal settings failed: {error}"))?;
            if let Some(object) = settings.as_object_mut() {
                object.insert("Id".to_string(), json!(settings_id));
            }
            let portal_port = self
                .portal
                .get_port()
                .await
                .map_err(|error| anyhow::anyhow!("portal port allocation failed: {error}"))?;
            let port = match self.config.game_port_override {
                Some(port) => {
                    if port != portal_port {
                        warn!(
                            portal_port,
                            port, "using the configured game port instead of the portal allocation"
                        );
                    }
                    port
                }
                None => portal_port,
            };
            info!(
                slave_id = registration.slave_id,
                slave_ip = %registration.slave_ip,
                settings_id, port, "portal setup complete"
            );
            *self.run.write().await = Some(RunContext {
                registration,
                settings,
                port,
            });
            self.process.start(port).await?;
            self.spawn_announcements();
        } else {
            info!(
                port = self.config.game_port,
                "portal registration disabled; starting a private server"
            );
            self.process.start(self.config.game_port).await?;
        }
        Ok(())
    }

    pub async fn settings(&self) -> Value {
        self.run
            .read()
            .await
            .as_ref()
            .map(|run| run.settings.clone())
            .unwrap_or_else(|| self.config.settings.clone())
    }

    pub async fn ranked(&self) -> bool {
        let settings = self.settings().await;
        settings::i64_field(&settings, &["RankedServer", "rankedServer"], 0) == 1
    }

    pub async fn portal_status(&self) -> Value {
        let registration = self.portal.registration().await;
        let run = self.run.read().await.clone();
        json!({
            "registered": registration.as_ref().is_some_and(|registration| registration.slave_id > 0),
            "slaveId": registration.as_ref().map(|registration| registration.slave_id),
            "slaveIp": registration.as_ref().map(|registration| registration.slave_ip.clone()),
            "quota": registration.as_ref().map(|registration| registration.dedi_quota),
            "settingsId": run.as_ref().map(|run| {
                settings::i64_field(&run.settings, &["Id", "id"], 0)
            }),
            "port": run.as_ref().map(|run| run.port),
        })
    }

    pub fn spawn_announcements(&self) {
        let state = self.clone();
        tokio::spawn(async move {
            loop {
                let Some(run) = state.run.read().await.clone() else {
                    tokio::time::sleep(Duration::from_secs(10)).await;
                    continue;
                };
                if let Some(payload) = state.announce_server_payload().await {
                    match state.portal.announce_server(&payload).await {
                        Ok(true) => {}
                        Ok(false) => warn!("the portal did not accept the server announcement"),
                        Err(error) => warn!(%error, "server announcement failed"),
                    }
                }
                tokio::time::sleep(run.registration.announce_interval()).await;
            }
        });
        let state = self.clone();
        tokio::spawn(async move {
            loop {
                let Some(run) = state.run.read().await.clone() else {
                    tokio::time::sleep(Duration::from_secs(10)).await;
                    continue;
                };
                let payload = state.announce_players_payload().await;
                if payload
                    .as_array()
                    .is_some_and(|players| !players.is_empty())
                    && let Err(error) = state.portal.announce_players(&payload).await
                {
                    warn!(%error, "player coordinate announcement failed");
                }
                tokio::time::sleep(run.registration.announce_players_interval()).await;
            }
        });
        let state = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(6 * 60 * 60)).await;
                if let Err(error) = state.portal.verify().await {
                    warn!(%error, "portal registration verification failed");
                }
            }
        });
    }

    /// An empty server list tells the portal to drop the entry.
    async fn announce_empty(&self) {
        let Some(run) = self.run.read().await.clone() else {
            return;
        };
        let payload = json!({
            "SlaveId": run.registration.slave_id,
            "AnnouncedTime": Utc::now().timestamp_millis(),
            "MpServerList": [],
        });
        if let Err(error) = self.portal.announce_server(&payload).await {
            warn!(%error, "failed to announce the empty server list");
        }
    }

    async fn announce_server_payload(&self) -> Option<Value> {
        let run = self.run.read().await.clone()?;
        let state = self.process.state().await;
        let mut servers = Vec::new();
        if state.running && state.last_state_at.is_some() {
            let settings = &run.settings;
            let layout = settings::track_layout(settings, 1);
            let sessions = layout.and_then(settings::sessions);
            let layout_ids: Vec<i64> =
                settings::array_field(settings, &["TrackLayouts", "trackLayouts"])
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(|layout| {
                                let id = settings::i64_field(layout, &["LayoutId", "layoutId"], 0);
                                (id > 0).then_some(id)
                            })
                            .collect()
                    })
                    .unwrap_or_default();
            let liveries: Vec<i64> = settings::array_field(settings, &["Liveries", "liveries"])
                .map(|items| items.iter().filter_map(Value::as_i64).collect())
                .unwrap_or_default();
            let players: Vec<i64> = state
                .players
                .iter()
                .filter_map(|player| {
                    settings::field(player, &["UserId", "userId"]).and_then(Value::as_i64)
                })
                .collect();
            let server_settings = json!({
                "Id": settings::i64_field(settings, &["Id", "id"], 0),
                "ServerName": settings::string_field(settings, &["Name", "name"], ""),
                "HasPassword": !settings::string_field(settings, &["Password", "password"], "").is_empty(),
                "Thumbnail": settings::field(settings, &["Thumbnail", "thumbnail"]).cloned().unwrap_or(Value::Null),
                "ExperienceId": settings::i64_field(settings, &["Experience", "experience"], 0),
                "TrackLayoutId": layout_ids,
                "LiveryId": liveries,
                "Difficulty": settings::i64_field(settings, &["DriveAssistPreset", "driveAssistPreset"], 0),
                "MaxNumberOfPlayers": settings::i64_field(settings, &["MaxNumOfPlayers", "maxNumOfPlayers"], 0),
                "FuelUsage": settings::i64_field(settings, &["FuelUsage", "fuelUsage"], 0),
                "TireWear": settings::i64_field(settings, &["TireWear", "tireWear"], 0),
                "MechanicalDamage": settings::i64_field(settings, &["Damage", "damage"], 0),
                "FlagRules": settings::i64_field(settings, &["FlagRules", "flagRules"], 0),
                "PracticeDuration": session_duration(sessions, "Practice"),
                "QualifyDuration": session_duration(sessions, "Qualify"),
                "QualifyStartMode": session_start_mode(sessions, "Qualify"),
                "Race1Duration": session_duration(sessions, "Race1"),
                "Race2Duration": session_duration(sessions, "Race2"),
                "Race3Duration": session_duration(sessions, "Race3"),
                "CutRules": settings::i64_field(settings, &["CutRules", "cutRules"], 0),
                "MandatoryPitStop": settings::i64_field(settings, &["MandatoryPitstop", "mandatoryPitstop"], 0),
                "ReverseGridPlaces": settings::i64_field(settings, &["ReverseStartOrderPlaces", "reverseStartOrderPlaces"], 0),
                "DriveAssist": {
                    "Preset": settings::i64_field(settings, &["DriveAssistPreset", "driveAssistPreset"], 0),
                    "Transmission": settings::i64_field(settings, &["DriveAssistTransmission", "driveAssistTransmission"], 0),
                    "Esp": settings::i64_field(settings, &["DriveAssistEsp", "driveAssistEsp"], 0),
                    "SteerAssist": settings::i64_field(settings, &["DriveAssistSteer", "driveAssistSteer"], 0),
                    "BrakeAssist": settings::i64_field(settings, &["DriveAssistBrake", "driveAssistBrake"], 0),
                    "TractionControl": settings::i64_field(settings, &["DriveAssistTractionControl", "driveAssistTractionControl"], 0),
                    "Raceline": settings::i64_field(settings, &["DriveAssistRaceline", "driveAssistRaceline"], 0),
                    "Autopit": settings::i64_field(settings, &["DriveAssistAutopit", "driveAssistAutopit"], 0),
                },
                "IsRanked": settings::i64_field(settings, &["RankedServer", "rankedServer"], 0) == 1,
                "MinRating": settings::f64_field(settings, &["MinRating", "minRating"], 0.0),
                "MaxRating": settings::f64_field(settings, &["MaxRating", "maxRating"], 0.0),
                "MinReputation": settings::f64_field(settings, &["MinReputation", "minReputation"], 0.0),
                "MaxReputation": settings::f64_field(settings, &["MaxReputation", "maxReputation"], 0.0),
                "PreventFalseStart": settings::i64_field(settings, &["PreventFalseStart", "preventFalseStart"], 0) == 1,
                "AllowQualJoin": settings::i64_field(settings, &["AllowPlayersJoinInQualifying", "allowPlayersJoinInQualifying"], 0) == 1,
                "BallastCarIds": [],
                "PowerRestrictionCarIds": [],
                "SlaveId": run.registration.slave_id,
            });
            servers.push(json!({
                "Id": settings::i64_field(settings, &["Id", "id"], 0),
                "GameSettings": settings,
                "Created": Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
                "ServerIp": run.registration.slave_ip.clone(),
                "Port": state.port,
                "PortTimeCritical": state.time_port,
                "CurrentSession": state.current_session,
                "TimeLeft": state.time_left,
                "PlayersOnServer": state.players_on_server,
                "Players": players,
                "Settings": server_settings,
            }));
        }
        Some(json!({
            "SlaveId": run.registration.slave_id,
            "AnnouncedTime": Utc::now().timestamp_millis(),
            "MpServerList": servers,
        }))
    }

    async fn announce_players_payload(&self) -> Value {
        let Some(run) = self.run.read().await.clone() else {
            return Value::Array(Vec::new());
        };
        let state = self.process.state().await;
        if !state.running || state.last_state_at.is_none() {
            return Value::Array(Vec::new());
        }
        Value::Array(vec![json!({
            "SlaveId": run.registration.slave_id,
            "ServerIp": run.registration.slave_ip,
            "Port": state.port,
            "Players": state.players,
        })])
    }

    /// Stores the decrypted result and pushes it to the metarace ingest
    /// endpoint. The game server is only acknowledged after delivery (D-084);
    /// a repeated callback with the same bytes reuses the stored file and
    /// retries a failed delivery instead of writing a duplicate.
    pub async fn handle_result(&self, payload: &Value) -> Result<PathBuf> {
        let body = serde_json::to_vec(payload).context("serialize the result")?;
        let hash = hex::encode(Sha256::digest(&body));

        let existing = self.results.lock().ok().and_then(|results| {
            results
                .get(&hash)
                .map(|entry| (entry.path.clone(), entry.delivered))
        });
        if let Some((path, delivered)) = existing {
            if delivered {
                info!(%hash, "duplicate result callback");
                return Ok(path);
            }
            info!(%hash, "retrying a previously failed result delivery");
            if let Some(target) = &self.config.result {
                self.push_result(target, &hash, &body).await?;
            }
            self.mark_delivered(&hash).await;
            return Ok(path);
        }

        let name = format!(
            "{}-{}.json",
            Utc::now().format("%Y%m%dT%H%M%SZ"),
            &hash[..12]
        );
        let path = self.config.results_dir.join(&name);
        tokio::fs::write(&path, &body)
            .await
            .with_context(|| format!("write result {}", path.display()))?;
        if let Ok(mut results) = self.results.lock() {
            results.insert(
                hash.clone(),
                StoredResult {
                    path: path.clone(),
                    delivered: false,
                },
            );
        }
        self.process.mark_result_received().await;

        if let Some(target) = &self.config.result {
            self.push_result(target, &hash, &body).await?;
        }
        self.mark_delivered(&hash).await;
        self.post_result_to_portal(payload, &hash).await;
        info!(path = %path.display(), "result stored");
        Ok(path)
    }

    async fn post_result_to_portal(&self, payload: &Value, hash: &str) {
        let Some(run) = self.run.read().await.clone() else {
            return;
        };
        let mut enriched = payload.clone();
        if let Some(object) = enriched.as_object_mut() {
            object.insert("SlaveId".to_string(), json!(run.registration.slave_id));
            object.insert("SlaveName".to_string(), json!(hostname()));
            object.insert("Created".to_string(), json!(Utc::now().timestamp()));
            object.insert(
                "CorrelationId".to_string(),
                json!(format!("{}-{}", Utc::now().timestamp_millis(), &hash[..12])),
            );
            object.insert("SpinNumber".to_string(), json!(1));
            object.insert("GameSettings".to_string(), run.settings.clone());
        }
        if let Err(error) = self.portal.post_result(&enriched).await {
            warn!(%error, "portal result post failed; the metarace result is stored");
        }
    }

    async fn mark_delivered(&self, hash: &str) {
        if let Ok(mut results) = self.results.lock()
            && let Some(entry) = results.get_mut(hash)
        {
            entry.delivered = true;
        }
        self.process.mark_result_delivered().await;
    }

    async fn push_result(
        &self,
        target: &crate::config::ResultTarget,
        hash: &str,
        body: &[u8],
    ) -> Result<()> {
        let mut delay = Duration::from_secs(1);
        let mut last_error = None;
        for attempt in 1..=target.attempts {
            let mut request = self
                .http
                .post(&target.url)
                .timeout(target.timeout)
                .bearer_auth(&target.token)
                .header("content-type", "application/json")
                .header("x-metarace-result-sha256", hash)
                .header(
                    "user-agent",
                    concat!("metarace-rre-manager/", env!("CARGO_PKG_VERSION")),
                )
                .body(body.to_vec());
            if let Some(run_id) = &target.run_id {
                request = request.header("x-metarace-run-id", run_id);
            }
            if let Some(server_id) = &target.server_id {
                request = request.header("x-metarace-server-id", server_id);
            }
            match request.send().await {
                Ok(response) if response.status().is_success() => {
                    info!(
                        status = %response.status(),
                        attempt,
                        "result accepted by the metarace API"
                    );
                    return Ok(());
                }
                Ok(response) => {
                    let status = response.status();
                    let text = response.text().await.unwrap_or_default();
                    warn!(%status, attempt, body = %text, "result upload rejected");
                    last_error = Some(anyhow::anyhow!("result upload rejected: {status}"));
                }
                Err(error) => {
                    warn!(%error, attempt, "result upload failed");
                    last_error = Some(anyhow::anyhow!(error));
                }
            }
            if attempt < target.attempts {
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(Duration::from_secs(16));
            }
        }
        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("result upload failed")))
    }

    pub async fn list_results(&self) -> Result<Vec<Value>> {
        let mut entries = Vec::new();
        let mut dir = tokio::fs::read_dir(&self.config.results_dir)
            .await
            .with_context(|| format!("read {}", self.config.results_dir.display()))?;
        while let Some(entry) = dir.next_entry().await? {
            let metadata = entry.metadata().await?;
            if !metadata.is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.ends_with(".json") {
                continue;
            }
            entries.push(json!({
                "name": name,
                "bytes": metadata.len(),
                "modified": metadata.modified().ok().and_then(|time| {
                    time.duration_since(std::time::UNIX_EPOCH).ok().map(|elapsed| elapsed.as_secs())
                }),
            }));
        }
        entries.sort_by(|left, right| {
            left["name"]
                .as_str()
                .unwrap_or_default()
                .cmp(right["name"].as_str().unwrap_or_default())
        });
        Ok(entries)
    }

    pub async fn read_result(&self, name: &str) -> Result<Vec<u8>> {
        if name.is_empty()
            || name.contains('/')
            || name.contains('\\')
            || name.contains("..")
            || !name.ends_with(".json")
        {
            bail!("invalid result file name");
        }
        let path = self.config.results_dir.join(name);
        tokio::fs::read(&path)
            .await
            .with_context(|| format!("read {}", path.display()))
    }

    /// Removes the server from the portal listing, then stops it.
    pub async fn stop(&self) -> Result<()> {
        self.announce_empty().await;
        self.process.stop().await
    }

    pub async fn shutdown(&self) {
        self.announce_empty().await;
        if let Err(error) = self.process.stop().await {
            warn!(%error, "failed to stop the dedicated server during shutdown");
        }
    }
}

fn session_duration(sessions: Option<&Map<String, Value>>, name: &str) -> i64 {
    sessions
        .and_then(|sessions| sessions.get(name))
        .map(|session| settings::i64_field(session, &["Duration", "duration"], 0))
        .unwrap_or(0)
}

fn session_start_mode(sessions: Option<&Map<String, Value>>, name: &str) -> i64 {
    sessions
        .and_then(|sessions| sessions.get(name))
        .map(|session| settings::i64_field(session, &["SessionType", "sessionType"], 0))
        .unwrap_or(0)
}

fn hostname() -> String {
    std::fs::read_to_string("/etc/hostname")
        .ok()
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .or_else(|| std::env::var("HOSTNAME").ok())
        .unwrap_or_else(|| "metarace".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::default_track_layout;

    #[test]
    fn empty_settings_default_to_unranked() {
        let settings = json!({ "TrackLayouts": [default_track_layout(1)] });
        assert_eq!(
            settings::i64_field(&settings, &["RankedServer", "rankedServer"], 0),
            0
        );
    }
}
