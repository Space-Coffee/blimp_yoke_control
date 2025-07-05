mod config_file;
mod sdl_joystick;
mod websocket;

use std::sync::Arc;

use tokio;

use crate::config_file::{read_config, ConfigFile};

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

#[tokio::main]
async fn main() -> tokio::io::Result<()> {
    println!("Hello, world!");

    let (shutdown_tx, mut shutdown_rx) = tokio::sync::broadcast::channel::<()>(8);

    let (yoke_tx, yoke_rx) = tokio::sync::mpsc::channel::<YokeEvent>(128);

    let config = Arc::new(read_config().await.unwrap());

    {
        let shutdown_tx = shutdown_tx.clone();
        let config = config.clone();
        std::thread::spawn(move || {
            sdl_joystick::sdl_thread(yoke_tx, shutdown_tx, config);
        });
    }

    websocket::ws_client_start(shutdown_tx.clone(), yoke_rx, config).await;

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            println!("Shutting down...");
            shutdown_tx.send(()).unwrap_or(0);
        }
        _ = shutdown_rx.recv() => {}
    };

    Ok(())
}
