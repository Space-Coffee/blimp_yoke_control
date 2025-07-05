use std::collections::BTreeMap;

use serde;
use serde_json;

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, serde::Deserialize, serde::Serialize)]
pub enum BlimpSteeringAxis {
    Throttle,
    Elevation,
    Yaw,
}

// Describes one physical axis mapped to one steering axis.
#[derive(serde::Deserialize, serde::Serialize)]
pub struct AxesMappingEntry {
    pub axis: BlimpSteeringAxis,
    pub keypoints: Vec<(i16, f32)>,
}

#[derive(serde::Deserialize, serde::Serialize)]
pub enum ButtonStyle {
    OnlyPress,
    OnlyRelease,
    PressAndRelease,
    Repeat(f32),
}

#[derive(serde::Deserialize, serde::Serialize)]
pub enum BlimpButtonFunction {
    FlightModeCycle,
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct ButtonMappingEntry {
    pub function: BlimpButtonFunction,
    pub style: ButtonStyle,
}

// This describes one virtual joystick or yoke.
// Our Turtle Beach yoke is detected as two devices.
#[derive(serde::Deserialize, serde::Serialize)]
pub struct AxesMappingPerJoy {
    pub name_regex: String,
    pub axes: BTreeMap<u8, AxesMappingEntry>,
    pub buttons: BTreeMap<u8, ButtonMappingEntry>,
}

// This describes an entire physical joystick or yoke.
#[derive(serde::Deserialize, serde::Serialize)]
pub struct ConfigFile {
    pub ws_addr: String,
    pub joys: Vec<AxesMappingPerJoy>,
}

pub async fn read_config() -> Result<ConfigFile, String> {
    serde_json::from_str::<ConfigFile>(
        &tokio::fs::read_to_string("config.json")
            .await
            .map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())
}
