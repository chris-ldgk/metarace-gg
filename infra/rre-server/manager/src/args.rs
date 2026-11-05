use anyhow::{Context, Result};
use serde_json::{Map, Value};

use crate::{
    config::Config,
    settings::{array_field, f64_field, field, i64_field, optional_string, sessions, track_layout},
};

/// Builds the dedicated-server argument vector for the metarace feature set:
/// one layout, one race, no portal, no rotation.
pub fn build_args(settings: &Value, port: i64, config: &Config) -> Result<Vec<String>> {
    if let Some(override_args) = std::env::var("METARACE_DEDI_ARGS")
        .ok()
        .filter(|value| !value.trim().is_empty())
    {
        return shell_words::split(&override_args)
            .context("METARACE_DEDI_ARGS must be shell-quoted");
    }

    let layout = track_layout(settings, 1)
        .context("server settings have no TrackLayouts; cannot start the dedicated server")?;
    let sessions = sessions(layout).context("track layout has no Sessions object")?;
    let practice = session(sessions, "Practice");
    let qualify = session(sessions, "Qualify");
    let race1 = session(sessions, "Race1");
    let race2 = session_optional(sessions, "Race2");
    let race3 = session_optional(sessions, "Race3");

    let mut args = Vec::new();

    if let Some(ip) = config.dedi_ip.clone().filter(|value| !value.is_empty()) {
        push(&mut args, "-ip");
        push(&mut args, &ip);
    }

    push_int(&mut args, "-port", port);
    push_int(&mut args, "-timePort", port + 2);

    let password = field(settings, &["Password", "password"])
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !password.is_empty() {
        push(&mut args, "-password");
        push(&mut args, password);
    }

    push_int(
        &mut args,
        "-experienceID",
        i64_field(settings, &["Experience", "experience"], 0),
    );
    push_int(
        &mut args,
        "-trackLayoutID",
        i64_field(layout, &["LayoutId", "layoutId"], 0),
    );

    if let Some(liveries) = array_field(settings, &["Liveries", "liveries"]) {
        let values: Vec<String> = liveries
            .iter()
            .filter_map(Value::as_i64)
            .map(|value| value.to_string())
            .collect();
        if !values.is_empty() {
            push(&mut args, "-carLiveryID");
            for value in values {
                args.push(value);
            }
        }
    }

    push_session_minutes(&mut args, "-practice1Minutes", practice);
    push_session_minutes(&mut args, "-qualify1Minutes", qualify);
    push_race(&mut args, "race1", race1);
    match race2 {
        Some(race) => push_race(&mut args, "race2", Some(race)),
        None => {
            push_int(&mut args, "-race2Minutes", 0);
            push_int(&mut args, "-race2Laps", 0);
        }
    }
    match race3 {
        Some(race) => push_race(&mut args, "race3", Some(race)),
        None => {
            push_int(&mut args, "-race3Minutes", 0);
            // Deliberately matches the original C# typo (race2Laps).
            push_int(&mut args, "-race2Laps", 0);
        }
    }

    push_int(
        &mut args,
        "-reverseStartOrderPlaces",
        i64_field(
            settings,
            &["ReverseStartOrderPlaces", "reverseStartOrderPlaces"],
            0,
        ),
    );
    push_int(
        &mut args,
        "-raceForceFinishTime",
        i64_field(settings, &["RaceFinishDuration", "raceFinishDuration"], 0),
    );
    push_int(
        &mut args,
        "-fuelUseage",
        i64_field(settings, &["FuelUsage", "fuelUsage"], 0),
    );
    push_int(
        &mut args,
        "-tireWear",
        i64_field(settings, &["TireWear", "tireWear"], 0),
    );
    push_int(
        &mut args,
        "-tireTemp",
        i64_field(settings, &["TireTemperature", "tireTemperature"], 0),
    );
    push_int(
        &mut args,
        "-flagRules",
        i64_field(settings, &["FlagRules", "flagRules"], 0),
    );
    push_int(
        &mut args,
        "-cutRules",
        i64_field(settings, &["CutRules", "cutRules"], 0),
    );
    push_int(
        &mut args,
        "-visualDamage",
        i64_field(settings, &["Damage", "damage"], 0),
    );
    push_int(
        &mut args,
        "-stateReportTime",
        i64_field(settings, &["StateReportInterval", "stateReportInterval"], 0),
    );
    push_int(&mut args, "-kickNonReadyClientsTime", 300);
    push_int(
        &mut args,
        "-compressionLevel",
        i64_field(settings, &["CompressionLevel", "compressionLevel"], 0),
    );
    push_int(
        &mut args,
        "-wreckerPrevention",
        i64_field(settings, &["WreckerPrevention", "wreckerPrevention"], 0),
    );
    push_int(
        &mut args,
        "-maxPlayers",
        i64_field(settings, &["MaxNumOfPlayers", "maxNumOfPlayers"], 24),
    );
    let warmup_minutes = sessions
        .get("WarmupDuration")
        .or_else(|| sessions.get("warmupDuration"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    push_int(&mut args, "-warmUpMinutes", warmup_minutes);
    push_int(
        &mut args,
        "-race1TimeOfDay",
        session_i64(race1, "TimeOfDay", 2),
    );
    push_int(
        &mut args,
        "-race1StartMode",
        session_i64(race1, "SessionType", 1),
    );

    push(&mut args, "-moodPerTODPerSession");
    for mood in moods(sessions) {
        args.push(mood.to_string());
    }

    push_int(
        &mut args,
        "-timeOfDayMultiplier",
        i64_field(layout, &["TimeOfDayMultiplier", "timeOfDayMultiplier"], 0),
    );

    if let Some(race) = race2 {
        push_int(
            &mut args,
            "-race2TimeOfDay",
            session_i64(Some(race), "TimeOfDay", 2),
        );
        push_int(
            &mut args,
            "-race2StartMode",
            session_i64(Some(race), "SessionType", 1),
        );
    }
    if let Some(race) = race3 {
        push_int(
            &mut args,
            "-race3TimeOfDay",
            session_i64(Some(race), "TimeOfDay", 2),
        );
        push_int(
            &mut args,
            "-race3StartMode",
            session_i64(Some(race), "SessionType", 1),
        );
    }
    push_int(
        &mut args,
        "-practice1TimeOfDay",
        session_i64(practice, "TimeOfDay", 2),
    );
    push_int(
        &mut args,
        "-qualify1TimeOfDay",
        session_i64(qualify, "TimeOfDay", 2),
    );
    push_int(
        &mut args,
        "-qualifyStartMode",
        session_i64(qualify, "SessionType", 1),
    );

    push_int(
        &mut args,
        "-mandatoryPitstop",
        i64_field(settings, &["MandatoryPitstop", "mandatoryPitstop"], 0),
    );
    push_int(
        &mut args,
        "-nonReadyPitlaneStartDelay",
        i64_field(
            settings,
            &["NonReadyPitlaneStartDelay", "nonReadyPitlaneStartDelay"],
            0,
        ),
    );
    push_int(
        &mut args,
        "-sessionFreeze",
        i64_field(
            settings,
            &[
                "StayInPracticeSessionIfEmpty",
                "stayInPracticeSessionIfEmpty",
            ],
            0,
        ),
    );
    push_int(
        &mut args,
        "-allowQualJoin",
        i64_field(
            settings,
            &[
                "AllowPlayersJoinInQualifying",
                "allowPlayersJoinInQualifying",
            ],
            0,
        ),
    );
    push_int(
        &mut args,
        "-raceForceCheckeredTime",
        i64_field(settings, &["CheckeredAfterTime", "checkeredAfterTime"], 0),
    );
    push_int(
        &mut args,
        "-forceFixedSetup",
        i64_field(settings, &["FixedSetup", "fixedSetup"], 0),
    );
    push_int(
        &mut args,
        "-maxIncidentPoints",
        i64_field(settings, &["MaxIncidentPoints", "maxIncidentPoints"], 0),
    );

    let max_ping = i64_field(settings, &["MaxPing", "maxPing"], 0);
    if max_ping != 0 {
        push_int(&mut args, "-maxPing", max_ping);
    }

    let da_preset = i64_field(settings, &["DriveAssistPreset", "driveAssistPreset"], 0);
    push_int(&mut args, "-daPreset", da_preset);
    if da_preset == 0 {
        push_int(
            &mut args,
            "-daTransmission",
            i64_field(
                settings,
                &["DriveAssistTransmission", "driveAssistTransmission"],
                -1,
            ),
        );
        push_int(
            &mut args,
            "-daEsp",
            i64_field(settings, &["DriveAssistEsp", "driveAssistEsp"], -1),
        );
        push_int(
            &mut args,
            "-daSteerAssist",
            i64_field(settings, &["DriveAssistSteer", "driveAssistSteer"], -1),
        );
        push_int(
            &mut args,
            "-daBrakeAssist",
            i64_field(settings, &["DriveAssistBrake", "driveAssistBrake"], -1),
        );
        push_int(
            &mut args,
            "-daTc",
            i64_field(
                settings,
                &["DriveAssistTractionControl", "driveAssistTractionControl"],
                -1,
            ),
        );
        push_int(
            &mut args,
            "-daRaceline",
            i64_field(
                settings,
                &["DriveAssistRaceline", "driveAssistRaceline"],
                -1,
            ),
        );
        push_int(
            &mut args,
            "-daAutopit",
            i64_field(settings, &["DriveAssistAutopit", "driveAssistAutopit"], -1),
        );
    }

    push_int(
        &mut args,
        "-preventFalseStart",
        i64_field(settings, &["PreventFalseStart", "preventFalseStart"], 0),
    );

    let event_pack_id = i64_field(layout, &["EventPackId", "eventPackId"], 0);
    if event_pack_id > 0 {
        push_int(&mut args, "-eventPackId", event_pack_id);
    }
    if let Some(ad_pack) = optional_string(settings, &["AdPack", "adPack"]) {
        push(&mut args, "-adPack");
        push(&mut args, &ad_pack);
    }

    push_float(
        &mut args,
        "-minRating",
        f64_field(settings, &["MinRating", "minRating"], 0.0),
    );
    push_float(
        &mut args,
        "-maxRating",
        f64_field(settings, &["MaxRating", "maxRating"], 0.0),
    );
    push_float(
        &mut args,
        "-minReputation",
        f64_field(settings, &["MinReputation", "minReputation"], 0.0),
    );
    push_float(
        &mut args,
        "-maxReputation",
        f64_field(settings, &["MaxReputation", "maxReputation"], 0.0),
    );

    push_int(
        &mut args,
        "-skipQualifying",
        i64_field(
            settings,
            &[
                "StartingGridFromLeaderboards",
                "startingGridFromLeaderboards",
            ],
            0,
        ),
    );
    push_int(
        &mut args,
        "-liveUpdate",
        i64_field(settings, &["LiveUpdate", "liveUpdate"], 0),
    );

    push(&mut args, "-nowindow");
    Ok(args)
}

fn session<'a>(sessions: &'a Map<String, Value>, name: &str) -> Option<&'a Value> {
    sessions
        .get(name)
        .or_else(|| sessions.get(&name.to_ascii_lowercase()))
        .filter(|value| !value.is_null())
}

fn session_optional<'a>(sessions: &'a Map<String, Value>, name: &str) -> Option<&'a Value> {
    session(sessions, name)
}

fn session_i64(session: Option<&Value>, name: &str, default: i64) -> i64 {
    session
        .map(|session| i64_field(session, &[name, &name.to_ascii_lowercase()], default))
        .unwrap_or(default)
}

fn push_session_minutes(args: &mut Vec<String>, flag: &str, session: Option<&Value>) {
    let minutes = session
        .map(|session| i64_field(session, &["Duration", "duration"], 0))
        .unwrap_or(0);
    push_int(args, flag, minutes);
}

fn push_race(args: &mut Vec<String>, prefix: &str, session: Option<&Value>) {
    let laps = session
        .map(|session| i64_field(session, &["Laps", "laps"], 0))
        .unwrap_or(0);
    let duration = session
        .map(|session| i64_field(session, &["Duration", "duration"], 0))
        .unwrap_or(0);
    if laps > 0 {
        push_int(args, &format!("-{prefix}Laps"), laps);
    } else {
        push_int(args, &format!("-{prefix}Minutes"), duration);
    }
}

fn moods(sessions: &Map<String, Value>) -> Vec<i64> {
    let mut out = Vec::with_capacity(25);
    for name in ["Practice", "Qualify", "Race1", "Race2", "Race3"] {
        let session = session(sessions, name);
        let clouds = session
            .and_then(|session| array_field(session, &["Clouds", "clouds"]))
            .filter(|values| !values.is_empty());
        match clouds {
            Some(values) => {
                out.extend(values.iter().filter_map(Value::as_i64));
            }
            None => out.extend(std::iter::repeat_n(0, 5)),
        }
    }
    out
}

fn push(args: &mut Vec<String>, value: &str) {
    args.push(value.to_string());
}

fn push_int(args: &mut Vec<String>, flag: &str, value: i64) {
    args.push(flag.to_string());
    args.push(value.to_string());
}

fn push_float(args: &mut Vec<String>, flag: &str, value: f64) {
    args.push(flag.to_string());
    args.push(format_float(value));
}

fn format_float(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{default_settings, default_track_layout};
    use serde_json::json;
    use std::path::PathBuf;

    fn config() -> Config {
        Config {
            bind: "127.0.0.1:8088".parse().unwrap(),
            data_dir: PathBuf::from("/tmp/metarace-test"),
            results_dir: PathBuf::from("/tmp/metarace-test/results"),
            settings: json!({}),
            game_port: 60000,
            game_port_override: None,
            dedi_ip: Some("10.0.0.5".to_string()),
            public_url: None,
            game_dir: None,
            game_zip: None,
            portal_secret: None,
            portal_registration_url: "http://127.0.0.1:1/register".to_string(),
            portal_disabled: true,
            game_launcher: "/bin/sh".to_string(),
            game_launcher_prefix_args: Vec::new(),
            wine_prefix: PathBuf::from("/tmp/metarace-test/prefix"),
            wineboot_bin: "wineboot".to_string(),
            wineserver_bin: "wineserver".to_string(),
            control_token: None,
            allow_insecure: true,
            result: None,
            stop_grace: std::time::Duration::from_secs(5),
            log_level: "error".to_string(),
        }
    }

    #[test]
    fn builds_core_arguments() {
        let mut settings = default_settings();
        settings["TrackLayouts"] = json!([default_track_layout(42)]);
        settings["Liveries"] = json!([11, 12]);
        settings["Password"] = json!("secret");
        settings["Experience"] = json!(3);
        settings["MaxNumOfPlayers"] = json!(16);
        settings["TrackLayouts"][0]["Sessions"]["Practice"]["Duration"] = json!(7);
        let args = build_args(&settings, 60000, &config()).unwrap();
        let text = args.join(" ");
        assert!(text.contains("-ip 10.0.0.5"));
        assert!(text.contains("-port 60000"));
        assert!(text.contains("-timePort 60002"));
        assert!(text.contains("-password secret"));
        assert!(text.contains("-trackLayoutID 42"));
        assert!(text.contains("-carLiveryID 11 12"));
        assert!(text.contains("-practice1Minutes 7"));
        assert!(text.contains("-maxPlayers 16"));
        assert!(text.contains("-nowindow"));
        assert!(!text.contains("-trackRotation"));
        assert!(!text.contains("-autoreconnect"));
    }

    #[test]
    fn refuses_settings_without_layouts() {
        let settings = default_settings();
        assert!(build_args(&settings, 60000, &config()).is_err());
    }

    #[test]
    fn leaves_out_empty_optional_values() {
        let mut settings = default_settings();
        settings["TrackLayouts"] = json!([default_track_layout(1)]);
        let args = build_args(&settings, 60000, &config()).unwrap();
        let text = args.join(" ");
        assert!(!text.contains("-password"));
        assert!(!text.contains("-carLiveryID"));
        assert!(!text.contains("-maxPing"));
        assert!(!text.contains("-adPack"));
    }

    #[test]
    fn writes_moods_for_every_session() {
        let mut settings = default_settings();
        settings["TrackLayouts"] = json!([default_track_layout(1)]);
        let args = build_args(&settings, 60000, &config()).unwrap();
        let index = args
            .iter()
            .position(|value| value == "-moodPerTODPerSession")
            .expect("mood flag");
        let moods: Vec<&String> = args[index + 1..index + 26].iter().collect();
        assert_eq!(moods.len(), 25);
        assert!(moods.iter().all(|value| value.as_str() == "0"));
    }
}
