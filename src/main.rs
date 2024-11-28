mod sdl_joystick;
mod websocket;

use futures_util::{SinkExt, StreamExt};
use sdl2;
use tokio;
use tokio_tungstenite;

use blimp_ground_ws_interface;

#[derive(Debug)]
enum YokeEvent {
    AxisMotion { axis: u8, value: i16 },
    ButtonState { button: u8, state: bool },
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
enum BlimpSteeringAxis {
    Throttle,
    Elevation,
    Yaw,
}

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

    crate::websocket::ws_client_start(shutdown_tx.clone(), yoke_rx).await;

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            println!("Shutting down...");
            shutdown_tx.send(()).unwrap_or(0);
        }
        _ = shutdown_rx.recv() => {}
    };

    Ok(())
}
