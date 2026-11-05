use std::{
    fs::{self, OpenOptions},
    path::PathBuf,
    process::Stdio,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use anyhow::{Context, Result, bail};
use chrono::Utc;
use serde::Serialize;
use serde_json::Value;
use tokio::{
    process::{Child, Command},
    sync::RwLock,
};
use tracing::{info, warn};

use crate::{args::build_args, config::Config, game::GameInstall, settings};

/// Everything the orchestrator needs to apply D-034: the process identity, the
/// last state callback and whether the result has been delivered.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessState {
    pub running: bool,
    pub pid: Option<u32>,
    pub port: i64,
    pub time_port: i64,
    pub players_on_server: i64,
    pub time_left: i64,
    pub session_time: i64,
    pub session_time_left: i64,
    pub current_session: i64,
    pub players: Vec<Value>,
    pub started_at: Option<i64>,
    pub last_state_at: Option<i64>,
    pub exit_code: Option<i32>,
    pub result_received_at: Option<i64>,
    pub result_delivered_at: Option<i64>,
}

#[derive(Clone)]
pub struct ProcessManager {
    config: Arc<Config>,
    state: Arc<RwLock<ProcessState>>,
    child: Arc<tokio::sync::Mutex<Option<Child>>>,
    stop_requested: Arc<AtomicBool>,
    started: Arc<AtomicBool>,
}

impl ProcessManager {
    pub fn new(config: Arc<Config>) -> Self {
        Self {
            state: Arc::new(RwLock::new(ProcessState {
                port: config.game_port,
                time_port: config.game_port + 2,
                ..Default::default()
            })),
            config,
            child: Arc::new(tokio::sync::Mutex::new(None)),
            stop_requested: Arc::new(AtomicBool::new(false)),
            started: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Spawns exactly one dedicated server and supervises it until it exits.
    /// A crashed server is not restarted: the orchestrator voids the split
    /// (D-008) and destroys the container.
    pub async fn start(&self, port: i64) -> Result<()> {
        if self.started.swap(true, Ordering::SeqCst) {
            bail!("the dedicated server has already been started");
        }
        let result = self.spawn(port).await;
        if result.is_err() {
            self.started.store(false, Ordering::SeqCst);
        }
        result
    }

    async fn spawn(&self, port: i64) -> Result<()> {
        let config = self.config.clone();
        let game = tokio::task::spawn_blocking(move || GameInstall::ensure(&config))
            .await
            .context("game payload extraction task failed")??;
        fs::write(
            game.workdir.join("MilkyWay.dedicated.config"),
            self.config.slave_url(),
        )
        .context("write MilkyWay.dedicated.config")?;
        if uses_wine(&self.config.game_launcher) {
            tokio::fs::create_dir_all(&self.config.wine_prefix)
                .await
                .context("create Wine prefix")?;
            self.ensure_wine_prefix().await;
        }

        let args = build_args(&self.config.settings, port, &self.config)?;
        tracing::debug!(
            args = ?redacted_args(&args),
            "dedicated server arguments"
        );
        let executable = std::env::var("METARACE_DEDI_EXE")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| game.exe.clone());
        let log_path = self
            .config
            .data_dir
            .join("logs")
            .join("dedicated-server.log");
        if let Some(parent) = log_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let log = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .with_context(|| format!("open {}", log_path.display()))?;
        let error_log = log.try_clone().context("clone log handle")?;

        let mut command = Command::new(&self.config.game_launcher);
        command.args(&self.config.game_launcher_prefix_args);
        command.arg(&executable);
        command.args(&args);
        command.current_dir(&game.workdir);
        command.env("WINEPREFIX", &self.config.wine_prefix);
        command.env(
            "WINEDEBUG",
            std::env::var("METARACE_WINEDEBUG").unwrap_or_else(|_| "-all".to_string()),
        );
        command.stdout(Stdio::from(log));
        command.stderr(Stdio::from(error_log));
        // The game server never needs the control or ingest credentials.
        command.env_remove("METARACE_CONTROL_TOKEN");
        command.env_remove("METARACE_RESULT_TOKEN");
        #[cfg(unix)]
        command.process_group(0);

        let child = command
            .spawn()
            .with_context(|| format!("spawn {}", executable.display()))?;
        let pid = child.id();
        info!(
            pid,
            port, log = %log_path.display(), "started the dedicated server"
        );
        {
            let mut guard = self.child.lock().await;
            *guard = Some(child);
        }
        {
            let mut state = self.state.write().await;
            state.running = true;
            state.pid = pid;
            state.port = port;
            state.time_port = port + 2;
            state.started_at = Some(Utc::now().timestamp());
            state.exit_code = None;
        }
        let manager = self.clone();
        tokio::spawn(async move {
            manager.supervise().await;
        });
        Ok(())
    }

    async fn supervise(&self) {
        loop {
            if self.stop_requested.load(Ordering::SeqCst) {
                break;
            }
            let exited = {
                let mut guard = self.child.lock().await;
                match guard.as_mut() {
                    Some(child) => match child.try_wait() {
                        Ok(Some(status)) => Some(status.code()),
                        Ok(None) => None,
                        Err(error) => {
                            warn!(%error, "cannot poll the dedicated server");
                            Some(None)
                        }
                    },
                    None => Some(None),
                }
            };
            match exited {
                None => tokio::time::sleep(Duration::from_millis(250)).await,
                Some(code) => {
                    let mut state = self.state.write().await;
                    state.running = false;
                    state.pid = None;
                    state.exit_code = code;
                    state.players.clear();
                    state.players_on_server = 0;
                    drop(state);
                    info!(exit_code = code, "the dedicated server exited");
                    break;
                }
            }
        }
    }

    /// Stops the game server and the Wine server for the prefix. Safe to call
    /// when nothing is running.
    pub async fn stop(&self) -> Result<()> {
        self.stop_requested.store(true, Ordering::SeqCst);
        self.terminate().await;
        self.kill_wine_prefix().await;
        {
            let mut state = self.state.write().await;
            state.running = false;
            state.pid = None;
            state.players.clear();
            state.players_on_server = 0;
        }
        self.started.store(false, Ordering::SeqCst);
        Ok(())
    }

    pub async fn state(&self) -> ProcessState {
        self.state.read().await.clone()
    }

    pub async fn update_from_slave(&self, payload: &Value) {
        let mut state = self.state.write().await;
        state.last_state_at = Some(Utc::now().timestamp());
        let port = settings::i64_field(payload, &["Port", "port"], 0);
        if port > 0 {
            state.port = port;
            state.time_port = settings::i64_field(payload, &["TimePort", "timePort"], port + 2);
        }
        state.current_session = settings::i64_field(
            payload,
            &["Session", "session", "CurrentSession", "currentSession"],
            state.current_session,
        );
        state.time_left = settings::i64_field(payload, &["TimeLeft", "timeLeft"], state.time_left);
        state.session_time =
            settings::i64_field(payload, &["SessionTime", "sessionTime"], state.session_time);
        state.session_time_left = settings::i64_field(
            payload,
            &["SessionTimeLeft", "sessionTimeLeft"],
            state.session_time_left,
        );
        if let Some(players) =
            settings::field(payload, &["Players", "players"]).and_then(Value::as_array)
        {
            state.players = players.clone();
            state.players_on_server = players.len() as i64;
        } else {
            state.players_on_server = settings::i64_field(
                payload,
                &[
                    "NumPlayers",
                    "numPlayers",
                    "PlayersOnServer",
                    "playersOnServer",
                ],
                state.players_on_server,
            );
        }
    }

    pub async fn mark_result_received(&self) {
        self.state.write().await.result_received_at = Some(Utc::now().timestamp());
    }

    pub async fn mark_result_delivered(&self) {
        self.state.write().await.result_delivered_at = Some(Utc::now().timestamp());
    }

    async fn terminate(&self) {
        let Some(pid) = self.child_pid().await else {
            return;
        };
        #[cfg(unix)]
        {
            use nix::{
                sys::signal::{Signal, kill},
                unistd::Pid,
            };
            let _ = kill(Pid::from_raw(-(pid as i32)), Signal::SIGTERM);
            let _ = kill(Pid::from_raw(pid as i32), Signal::SIGTERM);
        }
        let deadline = std::time::Instant::now() + self.config.stop_grace;
        loop {
            if self.try_wait_exited().await {
                break;
            }
            if std::time::Instant::now() >= deadline {
                warn!(pid, "the dedicated server did not stop; sending SIGKILL");
                #[cfg(unix)]
                {
                    use nix::{
                        sys::signal::{Signal, kill},
                        unistd::Pid,
                    };
                    let _ = kill(Pid::from_raw(-(pid as i32)), Signal::SIGKILL);
                    let _ = kill(Pid::from_raw(pid as i32), Signal::SIGKILL);
                }
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let mut guard = self.child.lock().await;
        if let Some(mut child) = guard.take() {
            let _ = child.start_kill();
            let _ = child.wait().await;
        }
    }

    async fn child_pid(&self) -> Option<u32> {
        self.child
            .lock()
            .await
            .as_ref()
            .and_then(|child| child.id())
    }

    async fn try_wait_exited(&self) -> bool {
        let mut guard = self.child.lock().await;
        match guard.as_mut() {
            Some(child) => !matches!(child.try_wait(), Ok(None)),
            None => true,
        }
    }

    async fn kill_wine_prefix(&self) {
        if !uses_wine(&self.config.game_launcher) {
            return;
        }
        let result = tokio::time::timeout(
            Duration::from_secs(15),
            Command::new(&self.config.wineserver_bin)
                .arg("-k")
                .env("WINEPREFIX", &self.config.wine_prefix)
                .env("WINEDEBUG", "-all")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status(),
        )
        .await;
        match result {
            Ok(Ok(status)) => {
                info!(
                    prefix = %self.config.wine_prefix.display(),
                    code = status.code(),
                    "stopped wineserver for the prefix"
                );
            }
            Ok(Err(error)) => warn!(%error, "wineserver -k failed"),
            Err(_) => warn!("wineserver -k timed out"),
        }
    }

    async fn ensure_wine_prefix(&self) {
        let prefix = &self.config.wine_prefix;
        if prefix.join("system.reg").exists() {
            return;
        }
        info!(prefix = %prefix.display(), "initializing the Wine prefix");
        // GE-Proton ships `wine`/`wineserver` but no `wineboot` wrapper; the
        // executable always accepts `wineboot` as its first argument.
        let mut command = Command::new(&self.config.wineboot_bin);
        if wine_binary(&self.config.wineboot_bin) {
            command.arg("wineboot");
        }
        command.arg("-u");
        command
            .env("WINEPREFIX", prefix)
            .env("WINEDEBUG", "-all")
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let result = tokio::time::timeout(Duration::from_secs(120), command.status()).await;
        match result {
            Ok(Ok(status)) if status.success() => {
                info!(prefix = %prefix.display(), "Wine prefix ready")
            }
            Ok(Ok(status)) => warn!(
                prefix = %prefix.display(),
                code = status.code(),
                "wineboot failed"
            ),
            Ok(Err(error)) => warn!(prefix = %prefix.display(), %error, "wineboot could not run"),
            Err(_) => warn!(prefix = %prefix.display(), "wineboot timed out"),
        }
    }
}

fn uses_wine(launcher: &str) -> bool {
    std::path::Path::new(launcher)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(launcher)
        .to_ascii_lowercase()
        .contains("wine")
}

fn wine_binary(path: &str) -> bool {
    matches!(
        std::path::Path::new(path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(path)
            .to_ascii_lowercase()
            .as_str(),
        "wine" | "wine64"
    )
}

fn redacted_args(args: &[String]) -> Vec<String> {
    let mut out = Vec::with_capacity(args.len());
    let mut hide_next = false;
    for arg in args {
        if hide_next {
            out.push("***".to_string());
            hide_next = false;
            continue;
        }
        out.push(arg.clone());
        hide_next = arg == "-password";
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{default_settings, default_track_layout};
    use serde_json::json;

    fn test_config(dir: &std::path::Path, launcher: &str) -> Config {
        let mut settings = default_settings();
        settings["TrackLayouts"] = json!([default_track_layout(1)]);
        Config {
            bind: "127.0.0.1:0".parse().unwrap(),
            data_dir: dir.to_path_buf(),
            results_dir: dir.join("results"),
            settings,
            game_port: 60000,
            game_port_override: None,
            dedi_ip: None,
            public_url: None,
            game_dir: Some(dir.join("game")),
            game_zip: None,
            portal_secret: None,
            portal_registration_url: "http://127.0.0.1:1/register".to_string(),
            portal_disabled: true,
            game_launcher: launcher.to_string(),
            game_launcher_prefix_args: Vec::new(),
            wine_prefix: dir.join("prefix"),
            wineboot_bin: "wineboot".to_string(),
            wineserver_bin: "wineserver".to_string(),
            control_token: Some("test".to_string()),
            allow_insecure: false,
            result: None,
            stop_grace: Duration::from_secs(2),
            log_level: "error".to_string(),
        }
    }

    #[tokio::test]
    async fn spawns_and_reports_the_exit() {
        let dir = tempfile::tempdir().unwrap();
        let game_dir = dir.path().join("game");
        std::fs::create_dir_all(&game_dir).unwrap();
        std::fs::write(game_dir.join("DedicatedServer.exe"), "#!/bin/sh\nexit 0\n").unwrap();
        let mut config = test_config(dir.path(), "/bin/sh");
        config.settings["TrackLayouts"] = json!([default_track_layout(1)]);
        let manager = ProcessManager::new(Arc::new(config));
        manager.start(60000).await.unwrap();
        assert!(
            manager.start(60000).await.is_err(),
            "a second start must fail"
        );
        let mut state = manager.state().await;
        for _ in 0..50 {
            state = manager.state().await;
            if !state.running {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert!(!state.running);
        assert_eq!(state.exit_code, Some(0));
        assert!(state.pid.is_none());
    }

    #[test]
    fn redacts_the_join_password() {
        let args = vec![
            "-port".to_string(),
            "60000".to_string(),
            "-password".to_string(),
            "secret".to_string(),
            "-maxPlayers".to_string(),
            "8".to_string(),
        ];
        let redacted = redacted_args(&args);
        assert_eq!(
            redacted,
            vec!["-port", "60000", "-password", "***", "-maxPlayers", "8"]
        );
    }

    #[tokio::test]
    async fn merges_slave_state() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path(), "/bin/true");
        let manager = ProcessManager::new(Arc::new(config));
        manager
            .update_from_slave(&json!({
                "Port": 60000,
                "TimePort": 60002,
                "Session": 2,
                "TimeLeft": 1234,
                "SessionTime": 60,
                "SessionTimeLeft": 30,
                "Players": [{ "UserId": 7 }]
            }))
            .await;
        let state = manager.state().await;
        assert_eq!(state.current_session, 2);
        assert_eq!(state.time_left, 1234);
        assert_eq!(state.players_on_server, 1);
        assert!(state.last_state_at.is_some());
    }
}
