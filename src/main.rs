mod sdl_joystick;
mod websocket;

use std::collections::BTreeMap;
use std::sync::Arc;

use serde;
use serde_json;
use tokio;

#[derive(Debug)]
enum YokeEvent {
    AxisMotion {
        joy_id: u32,
        axis: u8,
        value: i16,
    },
    ButtonState {
        joy_id: u32,
        button: u8,
        state: bool,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, serde::Deserialize, serde::Serialize)]
enum BlimpSteeringAxis {
    Throttle,
    Elevation,
    Yaw,
}

// Describes one physical axis mapped to one steering axis.
#[derive(serde::Deserialize, serde::Serialize)]
struct AxesMappingEntry(BlimpSteeringAxis, i16, i16);

// This describes one virtual joystick or yoke.
// Our Turtle Beach yoke is detected as two devices.
#[derive(serde::Deserialize, serde::Serialize)]
struct AxesMappingPerJoy {
    pub name_regex: String,
    pub axes: BTreeMap<u8, AxesMappingEntry>,
}

// This describes an entire physical joystick or yoke.
#[derive(serde::Deserialize, serde::Serialize)]
struct AxesMapping {
    pub joys: Vec<AxesMappingPerJoy>,
}

#[tokio::main]
async fn main() -> tokio::io::Result<()> {
    println!("Hello, world!");

    let (shutdown_tx, mut shutdown_rx) = tokio::sync::broadcast::channel::<()>(8);

    let (yoke_tx, yoke_rx) = tokio::sync::mpsc::channel::<YokeEvent>(128);

    let mapping = Arc::new(
        serde_json::from_str::<AxesMapping>(
            &tokio::fs::read_to_string("mapping.json")
                .await
                .expect("File mapping.json not found"),
        )
        .expect("Invalid JSON file mapping.json"),
    );

    {
        let shutdown_tx = shutdown_tx.clone();
        let mapping = mapping.clone();
        std::thread::spawn(move || {
            crate::sdl_joystick::sdl_thread(yoke_tx, shutdown_tx, mapping);
        });
    }

    crate::websocket::ws_client_start(shutdown_tx.clone(), yoke_rx, mapping).await;

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            println!("Shutting down...");
            shutdown_tx.send(()).unwrap_or(0);
        }
        _ = shutdown_rx.recv() => {}
    };

    Ok(())
}
