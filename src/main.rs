use futures_util::SinkExt;
use sdl2;
use tokio;
use tokio_tungstenite;

use blimp_ground_ws_interface;

#[derive(Debug)]
enum YokeEvent {
    AxisMotion { axis: u8, value: i16 },
    ButtonState { button: u8, state: bool },
}

fn sdl_thread(
    yoke_tx: tokio::sync::mpsc::Sender<YokeEvent>,
    mut shutdown_rx: tokio::sync::broadcast::Receiver<()>,
) {
    let sdl_ctx = sdl2::init().unwrap();
    let mut event_pump = sdl_ctx.event_pump().unwrap();
    let joystick_subsys = sdl_ctx.joystick().unwrap();
    let joystick = joystick_subsys.open(0).expect("Couldn't open joystick");

    'ev_loop: loop {
        match shutdown_rx.try_recv() {
            Ok(_) | Err(tokio::sync::broadcast::error::TryRecvError::Closed) => {
                break;
            }
            _ => {}
        }
        for ev in event_pump.poll_iter() {
            match ev {
                sdl2::event::Event::Quit { .. }
                | sdl2::event::Event::KeyDown {
                    keycode: Some(sdl2::keyboard::Keycode::Escape),
                    ..
                } => {
                    break 'ev_loop;
                }

                sdl2::event::Event::JoyAxisMotion {
                    timestamp,
                    which,
                    axis_idx,
                    value,
                } => {
                    yoke_tx
                        .blocking_send(YokeEvent::AxisMotion {
                            axis: axis_idx,
                            value,
                        })
                        .unwrap();
                }
                sdl2::event::Event::JoyButtonDown {
                    timestamp,
                    which,
                    button_idx,
                } => {
                    yoke_tx
                        .blocking_send(YokeEvent::ButtonState {
                            button: button_idx,
                            state: true,
                        })
                        .unwrap();
                }
                sdl2::event::Event::JoyButtonUp {
                    timestamp,
                    which,
                    button_idx,
                } => {
                    yoke_tx
                        .blocking_send(YokeEvent::ButtonState {
                            button: button_idx,
                            state: false,
                        })
                        .unwrap();
                }
                _ => {}
            }
        }
    }
}

#[tokio::main]
async fn main() -> tokio::io::Result<()> {
    println!("Hello, world!");

    let (shutdown_tx, mut shutdown_rx) = tokio::sync::broadcast::channel::<()>(1);

    let (yoke_tx, mut yoke_rx) = tokio::sync::mpsc::channel::<YokeEvent>(128);

    {
        let mut shutdown_rx = shutdown_tx.subscribe();
        std::thread::spawn(move || {
            sdl_thread(yoke_tx, shutdown_rx);
        });
    }

    let (mut ws_stream, _) = tokio_tungstenite::connect_async(
        tokio_tungstenite::tungstenite::client::IntoClientRequest::into_client_request(
            "ws://127.0.0.1:8765",
        )
        .unwrap(),
    )
    .await
    .expect("Couldn't open WebSocket");
    println!("Opened WebSocket connection");

    tokio::spawn(async move {
        loop {
            tokio::select! {
                yoke_ev = yoke_rx.recv() => {
                    println!("{:?}", yoke_ev);
                    match yoke_ev {
                        Some(YokeEvent::AxisMotion { axis, value }) => {},
                        Some(YokeEvent::ButtonState { button, state }) => {},
                        None => {
                            break;
                        }
                    }
                    ws_stream
                        .send(tokio_tungstenite::tungstenite::Message::Binary(
                            postcard::to_stdvec::<blimp_ground_ws_interface::MessageV2G>(
                                &blimp_ground_ws_interface::MessageV2G::Controls(
                                    blimp_ground_ws_interface::Controls {
                                        throttle: 0,
                                        pitch: 0,
                                        roll: 0,
                                    },
                                ),
                            )
                            .unwrap(),
                        ))
                        .await
                        .unwrap();
                }
                _ = shutdown_rx.recv() => {
                    break;
                }
            }
        }
    });

    tokio::signal::ctrl_c().await.unwrap();
    println!("Shutting down...");
    shutdown_tx.send(()).unwrap();

    Ok(())
}
