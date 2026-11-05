use std::{env, net::SocketAddr, path::PathBuf, time::Duration};

use anyhow::{Context, Result, bail};
use serde_json::Value;

use crate::settings;

pub const DEFAULT_GAME_PORT: i64 = 60000;

pub fn default_portal_registration_url() -> String {
    format!(
        "https://mw.raceroomracing.com/RRRMPService/v{}/RRRCommunityMultiplayerService.svc/RegisterCommunityMpSlave",
        crate::embedded::ASSEMBLY_MAJOR
    )
}

#[derive(Clone, Debug)]
pub struct ResultTarget {
    pub url: String,
    pub token: String,
    pub run_id: Option<String>,
    pub server_id: Option<String>,
    pub attempts: usize,
    pub timeout: Duration,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub bind: SocketAddr,
    pub data_dir: PathBuf,
    pub results_dir: PathBuf,
    pub settings: Value,

    pub game_port: i64,
    pub game_port_override: Option<i64>,
    pub dedi_ip: Option<String>,
    pub public_url: Option<String>,
    pub game_dir: Option<PathBuf>,
    pub game_zip: Option<PathBuf>,

    pub portal_secret: Option<String>,
    pub portal_registration_url: String,
    pub portal_disabled: bool,

    pub game_launcher: String,
    pub game_launcher_prefix_args: Vec<String>,
    pub wine_prefix: PathBuf,
    pub wineboot_bin: String,
    pub wineserver_bin: String,

    pub control_token: Option<String>,
    pub allow_insecure: bool,
    pub result: Option<ResultTarget>,
    pub stop_grace: Duration,
    pub log_level: String,
}

impl Config {
    pub fn load() -> Result<Self> {
        let data_dir = env_path("METARACE_DATA_DIR").unwrap_or_else(|| PathBuf::from("/data"));
        let results_dir =
            env_path("METARACE_RESULTS_DIR").unwrap_or_else(|| data_dir.join("results"));
        let settings = load_settings()?;
        validate_settings(&settings)?;

        let bind = env_string("METARACE_BIND")
            .unwrap_or_else(|| "0.0.0.0:8088".to_string())
            .parse::<SocketAddr>()
            .context("METARACE_BIND must be host:port")?;

        let game_port_override = env_string("METARACE_GAME_PORT")
            .map(|value| value.parse::<i64>())
            .transpose()
            .context("METARACE_GAME_PORT must be an integer")?;
        let game_port = game_port_override.unwrap_or(DEFAULT_GAME_PORT);
        if !(1..=65533).contains(&game_port) {
            bail!("METARACE_GAME_PORT must be between 1 and 65533");
        }

        let portal_disabled = env_bool("METARACE_PORTAL_DISABLED", false);
        let portal_secret = env_string("METARACE_PORTAL_SECRET");
        if !portal_disabled && portal_secret.is_none() {
            bail!(
                "METARACE_PORTAL_SECRET is required for server-browser registration; set METARACE_PORTAL_DISABLED=true only for local testing"
            );
        }
        let portal_registration_url = env_string("METARACE_PORTAL_REGISTRATION_URL")
            .unwrap_or_else(default_portal_registration_url);

        let control_token = env_string("METARACE_CONTROL_TOKEN");
        let allow_insecure = env_bool("METARACE_ALLOW_INSECURE", false);
        if control_token.is_none() && !allow_insecure {
            bail!(
                "METARACE_CONTROL_TOKEN is required; set METARACE_ALLOW_INSECURE=true only for local testing"
            );
        }

        let result = match env_string("METARACE_RESULT_URL") {
            Some(url) => Some(ResultTarget {
                url,
                token: env_string("METARACE_RESULT_TOKEN")
                    .context("METARACE_RESULT_TOKEN is required with METARACE_RESULT_URL")?,
                run_id: env_string("METARACE_RUN_ID"),
                server_id: env_string("METARACE_SERVER_ID"),
                attempts: env_string("METARACE_RESULT_ATTEMPTS")
                    .map(|value| value.parse::<usize>())
                    .transpose()
                    .context("METARACE_RESULT_ATTEMPTS must be an integer")?
                    .unwrap_or(5)
                    .max(1),
                timeout: Duration::from_secs(
                    env_string("METARACE_RESULT_TIMEOUT_SECS")
                        .map(|value| value.parse::<u64>())
                        .transpose()
                        .context("METARACE_RESULT_TIMEOUT_SECS must be an integer")?
                        .unwrap_or(15),
                ),
            }),
            None => None,
        };

        let config = Self {
            bind,
            results_dir,
            settings,
            game_port,
            game_port_override,
            // The game server binds IPv6-only when it has no explicit IPv4.
            // Docker and the gateway publish IPv4, so default to the
            // container's primary IPv4 and let METARACE_DEDI_IP override it.
            dedi_ip: env_string("METARACE_DEDI_IP").or_else(detect_local_ipv4),
            public_url: env_string("METARACE_PUBLIC_URL"),
            game_dir: env_path("METARACE_GAME_DIR"),
            game_zip: env_path("METARACE_GAME_ZIP"),
            portal_secret,
            portal_registration_url,
            portal_disabled,
            game_launcher: env_string("METARACE_GAME_LAUNCHER")
                .unwrap_or_else(|| "wine".to_string()),
            game_launcher_prefix_args: env_string("METARACE_GAME_LAUNCHER_PREFIX_ARGS")
                .map(|value| shell_words::split(&value))
                .transpose()
                .context("METARACE_GAME_LAUNCHER_PREFIX_ARGS must be shell-quoted")?
                .unwrap_or_default(),
            wine_prefix: env_path("METARACE_WINE_PREFIX")
                .unwrap_or_else(|| data_dir.join("wineprefix")),
            wineboot_bin: env_string("METARACE_WINEBOOT_BIN")
                .unwrap_or_else(|| "wineboot".to_string()),
            wineserver_bin: env_string("METARACE_WINESERVER_BIN")
                .unwrap_or_else(|| "wineserver".to_string()),
            control_token,
            allow_insecure,
            result,
            stop_grace: Duration::from_secs(
                env_string("METARACE_STOP_GRACE_SECS")
                    .map(|value| value.parse::<u64>())
                    .transpose()
                    .context("METARACE_STOP_GRACE_SECS must be an integer")?
                    .unwrap_or(20),
            ),
            log_level: env_string("METARACE_LOG_LEVEL").unwrap_or_else(|| "info".to_string()),
            data_dir,
        };

        std::fs::create_dir_all(&config.data_dir)
            .with_context(|| format!("create data dir {}", config.data_dir.display()))?;
        std::fs::create_dir_all(&config.results_dir)
            .with_context(|| format!("create results dir {}", config.results_dir.display()))?;
        Ok(config)
    }

    /// Base URL written into `MilkyWay.dedicated.config`. The game server runs
    /// in the same container, so loopback is always reachable.
    pub fn slave_url(&self) -> String {
        match &self.public_url {
            Some(url) => {
                let base = url.trim_end_matches('/');
                if base.ends_with("/slave") {
                    format!("{base}/")
                } else {
                    format!("{base}/slave/")
                }
            }
            None => format!("http://127.0.0.1:{}/slave/", self.bind.port()),
        }
    }

    pub fn game_extract_dir(&self) -> PathBuf {
        self.data_dir
            .join("game")
            .join(crate::embedded::ASSEMBLY_VERSION)
    }

    pub fn game_source(&self) -> GameSource {
        if let Some(dir) = &self.game_dir {
            return GameSource::Dir(dir.clone());
        }
        if let Some(zip) = &self.game_zip {
            return GameSource::Zip(zip.clone());
        }
        GameSource::Embedded
    }

    pub fn result_enabled(&self) -> bool {
        self.result.is_some()
    }

    pub fn portal_enabled(&self) -> bool {
        !self.portal_disabled
    }
}

#[derive(Clone, Debug)]
pub enum GameSource {
    Dir(PathBuf),
    Zip(PathBuf),
    Embedded,
}

fn load_settings() -> Result<Value> {
    if let Some(path) = env_path("METARACE_SETTINGS_FILE") {
        return settings::load_json(&path)
            .with_context(|| format!("load METARACE_SETTINGS_FILE {}", path.display()));
    }
    if let Some(raw) = env_string("METARACE_SETTINGS") {
        return serde_json::from_str(&raw).context("METARACE_SETTINGS must be a JSON object");
    }
    bail!("set METARACE_SETTINGS_FILE or METARACE_SETTINGS to the RRE server settings JSON")
}

fn validate_settings(settings: &Value) -> Result<()> {
    if !settings.is_object() {
        bail!("server settings must be a JSON object");
    }
    let layouts = settings::array_field(settings, &["TrackLayouts", "trackLayouts"])
        .context("server settings must contain a TrackLayouts array")?;
    if layouts.is_empty() {
        bail!("server settings must contain at least one TrackLayouts entry");
    }
    let sessions =
        settings::sessions(&layouts[0]).context("track layout has no Sessions object")?;
    for name in ["Practice", "Qualify", "Race1"] {
        if sessions
            .get(name)
            .filter(|value| !value.is_null())
            .is_none()
        {
            bail!("track layout has no {name} session");
        }
    }
    Ok(())
}

fn env_string(key: &str) -> Option<String> {
    env::var(key).ok().filter(|value| !value.trim().is_empty())
}

fn env_path(key: &str) -> Option<PathBuf> {
    env_string(key).map(PathBuf::from)
}

fn env_bool(key: &str, default: bool) -> bool {
    match env_string(key) {
        Some(value) => matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        None => default,
    }
}

fn detect_local_ipv4() -> Option<String> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("1.1.1.1:80").ok()?;
    let ip = socket.local_addr().ok()?.ip();
    ip.is_ipv4().then(|| ip.to_string())
}
