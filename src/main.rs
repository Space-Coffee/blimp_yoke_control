mod sdl_joystick;
mod websocket;

use std::collections::BTreeMap;
use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use sdl2;
use serde;
use serde_json;
use tokio;
use tokio_tungstenite;

use blimp_ground_ws_interface;

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

#[derive(serde::Deserialize, serde::Serialize)]
struct AxesMappingEntry(BlimpSteeringAxis, i16, i16);

#[derive(serde::Deserialize, serde::Serialize)]
struct AxesMappingPerJoy {
    pub name_regex: String,
    pub axes: BTreeMap<u8, AxesMappingEntry>,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct AxesMapping {
    pub joys: Vec<AxesMappingPerJoy>,
}

#[tokio::main]
async fn main() -> tokio::io::Result<()> {
    println!("Hello, world!");

    let (shutdown_tx, mut shutdown_rx) = tokio::sync::broadcast::channel::<()>(8);

    let (yoke_tx, mut yoke_rx) = tokio::sync::mpsc::channel::<YokeEvent>(128);

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
