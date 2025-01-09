mod sdl_joystick;
mod websocket;

use futures_util::{SinkExt, StreamExt};
use sdl2;
use serde;
use serde_json;
use tokio;
use tokio_tungstenite;

use blimp_ground_ws_interface;

#[derive(Debug)]
enum YokeEvent {
    AxisMotion { axis: u8, value: i16 },
    ButtonState { button: u8, state: bool },
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, serde::Deserialize, serde::Serialize)]
enum BlimpSteeringAxis {
    Throttle,
    Elevation,
    Yaw,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct AxesMappingEntry(BlimpSteeringAxis, i16, i16);

#[tokio::main]
async fn main() -> tokio::io::Result<()> {
    println!("Hello, world!");

    let (shutdown_tx, mut shutdown_rx) = tokio::sync::broadcast::channel::<()>(8);

    let (yoke_tx, mut yoke_rx) = tokio::sync::mpsc::channel::<YokeEvent>(128);

    {
        let shutdown_tx = shutdown_tx.clone();
        std::thread::spawn(move || {
            crate::sdl_joystick::sdl_thread(yoke_tx, shutdown_tx);
        });
    }

    //let mut axes_mapping = std::collections::BTreeMap::<u8, AxesMappingEntry>::new();
    //axes_mapping.insert(
    //    1,
    //    AxesMappingEntry(BlimpSteeringAxis::Throttle, 32767, -32768),
    //);
    //axes_mapping.insert(0, AxesMappingEntry(BlimpSteeringAxis::Yaw, -32768, 32767));
    //axes_mapping.insert(
    //    4,
    //    AxesMappingEntry(BlimpSteeringAxis::Elevation, 32767, -32768),
    //);
    //println!("{}", serde_json::to_string(&axes_mapping).unwrap());

    let axes_mapping = serde_json::from_str::<std::collections::BTreeMap<u8, AxesMappingEntry>>(
        &tokio::fs::read_to_string("mapping.json")
            .await
            .expect("File mapping.json not found"),
    )
    .expect("Invalid JSON file mapping.json");

    crate::websocket::ws_client_start(shutdown_tx.clone(), yoke_rx, axes_mapping).await;

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            println!("Shutting down...");
            shutdown_tx.send(()).unwrap_or(0);
        }
        _ = shutdown_rx.recv() => {}
    };

    Ok(())
}
