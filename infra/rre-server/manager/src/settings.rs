use std::path::Path;

use serde_json::{Map, Value};

/// Reads a field by any of the supplied names (PascalCase and camelCase are
/// both common in the settings payloads).
pub fn field<'a>(value: &'a Value, names: &[&str]) -> Option<&'a Value> {
    let object = value.as_object()?;
    for name in names {
        if let Some(value) = object.get(*name)
            && !value.is_null()
        {
            return Some(value);
        }
    }
    None
}

pub fn optional_string(value: &Value, names: &[&str]) -> Option<String> {
    field(value, names)
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

pub fn i64_field(value: &Value, names: &[&str], default: i64) -> i64 {
    field(value, names)
        .and_then(|v| v.as_i64().or_else(|| v.as_u64().map(|n| n as i64)))
        .unwrap_or(default)
}

pub fn string_field(value: &Value, names: &[&str], default: &str) -> String {
    field(value, names)
        .and_then(Value::as_str)
        .unwrap_or(default)
        .to_string()
}

pub fn bool_field(value: &Value, names: &[&str], default: bool) -> bool {
    field(value, names)
        .and_then(Value::as_bool)
        .unwrap_or(default)
}

pub fn f64_field(value: &Value, names: &[&str], default: f64) -> f64 {
    field(value, names)
        .and_then(|v| {
            v.as_f64()
                .or_else(|| v.as_i64().map(|n| n as f64))
                .or_else(|| v.as_u64().map(|n| n as f64))
        })
        .unwrap_or(default)
}

pub fn array_field<'a>(value: &'a Value, names: &[&str]) -> Option<&'a Vec<Value>> {
    field(value, names).and_then(Value::as_array)
}

pub fn object_field<'a>(value: &'a Value, names: &[&str]) -> Option<&'a Map<String, Value>> {
    field(value, names).and_then(Value::as_object)
}

pub fn load_json(path: &Path) -> anyhow::Result<Value> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| anyhow::anyhow!("read {}: {error}", path.display()))?;
    Ok(serde_json::from_str(&text)?)
}

pub fn track_layout(settings: &Value, spin_number: usize) -> Option<&Value> {
    let layouts = array_field(settings, &["TrackLayouts", "trackLayouts"])?;
    if layouts.is_empty() {
        return None;
    }
    let index = spin_number.saturating_sub(1) % layouts.len();
    Some(&layouts[index])
}

pub fn sessions(layout: &Value) -> Option<&Map<String, Value>> {
    object_field(layout, &["Sessions", "sessions"])
}

#[cfg(test)]
pub fn default_settings() -> Value {
    serde_json::json!({
        "Id": 1,
        "Name": "",
        "Password": "",
        "Experience": 0,
        "DriveAssistPreset": 0,
        "DriveAssistTransmission": -1,
        "DriveAssistEsp": -1,
        "DriveAssistSteer": -1,
        "DriveAssistBrake": -1,
        "DriveAssistTractionControl": -1,
        "DriveAssistRaceline": -1,
        "DriveAssistAutopit": -1,
        "TrackLayouts": [],
        "ReverseStartOrderPlaces": 0,
        "Cars": [],
        "Liveries": [],
        "MaxNumOfPlayers": 24,
        "FuelUsage": 0,
        "TireWear": 0,
        "TireTemperature": 0,
        "Damage": 0,
        "RaceFinishDuration": 0,
        "FlagRules": 0,
        "CutRules": 0,
        "StateReportInterval": 0,
        "CompressionLevel": 0,
        "WreckerPrevention": 0,
        "MandatoryPitstop": 0,
        "NonReadyPitlaneStartDelay": 0,
        "StayInPracticeSessionIfEmpty": 0,
        "AllowPlayersJoinInQualifying": 0,
        "StartingGridFromLeaderboards": 0,
        "CheckeredAfterTime": 0,
        "MaxPing": 0,
        "FixedSetup": 0,
        "AdPack": null,
        "RankedServer": 0,
        "MaxIncidentPoints": 0,
        "MinRating": 0,
        "MaxRating": 0,
        "MinReputation": 0,
        "MaxReputation": 0,
        "PreventFalseStart": 0,
        "LiveUpdate": 0
    })
}

#[cfg(test)]
pub fn default_session(duration: i64) -> Value {
    serde_json::json!({
        "Duration": duration,
        "Laps": 0,
        "TimeOfDay": 2,
        "SessionType": 1
    })
}

#[cfg(test)]
pub fn default_track_layout(layout_id: i64) -> Value {
    serde_json::json!({
        "LayoutId": layout_id,
        "Sessions": {
            "WarmupDuration": 0,
            "Practice": default_session(5),
            "Qualify": default_session(10),
            "Race1": default_session(20),
            "Race2": null,
            "Race3": null
        },
        "TimeOfDayMultiplier": 0,
        "EventPackId": 0
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_pascal_and_camel() {
        let value = serde_json::json!({ "ProcessId": 7, "Name": "x", "trackLayouts": [] });
        assert_eq!(i64_field(&value, &["ProcessId", "processId"], 0), 7);
        assert_eq!(
            optional_string(&value, &["Name", "name"]).as_deref(),
            Some("x")
        );
        assert!(array_field(&value, &["TrackLayouts", "trackLayouts"]).is_some());
    }

    #[test]
    fn default_layout_is_usable() {
        let layout = default_track_layout(42);
        assert_eq!(i64_field(&layout, &["LayoutId"], 0), 42);
        assert!(sessions(&layout).is_some());
    }
}
