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

    //let ws_stream = std::sync::Arc::new(tokio::sync::Mutex::new(ws_stream));

    let (mut ws_stream_tx, mut ws_stream_rx) = ws_stream.split();

    ws_stream_tx
        .send(tokio_tungstenite::tungstenite::Message::Binary(
            postcard::to_stdvec::<blimp_ground_ws_interface::MessageV2G>(
                &blimp_ground_ws_interface::MessageV2G::DeclareInterest(
                    blimp_ground_ws_interface::VisInterest {
                        motors: true,
                        servos: false,
                    },
                ),
            )
            .unwrap(),
        ))
        .await
        .unwrap();

    {
        let mut shutdown_rx = shutdown_tx.subscribe();
        //let ws_stream = ws_stream.clone();
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
                        let msg_ser = postcard::to_stdvec::<blimp_ground_ws_interface::MessageV2G>(
                                    &blimp_ground_ws_interface::MessageV2G::Controls(
                                        blimp_ground_ws_interface::Controls {
                                            throttle: 0,
                                            pitch: 0,
                                            roll: 0,
                                        },
                                    ),
                                ).unwrap();
                        //println!("Serialized message");
                        //let mut ws_stream_locked = ws_stream.lock().await;
                        //println!("Locked WS stream");
                        ws_stream_tx
                            .send(tokio_tungstenite::tungstenite::Message::Binary(
                               msg_ser,
                            ))
                            .await
                            .unwrap();
                        //println!("Send message");
                    }
                    _ = shutdown_rx.recv() => {
                        break;
                    }
                }
                //tokio::task::yield_now().await;
            }
        });
    }

    tokio::spawn(async move {
        loop {
            //let mut ws_stream_locked = ws_stream.lock().await;
            tokio::select! {
                ws_msg = ws_stream_rx.next() => {
                    //println!("Received some WS message: {:#?}", ws_msg);
                    if let Some(ws_msg) = ws_msg{
                        if let Ok(ws_msg) = ws_msg{
                            println!("Got WS G2V message: {:#?}", ws_msg);
                            match ws_msg {
                                tokio_tungstenite::tungstenite::Message::Binary(msg_bin) => {
                                    let msg_des = postcard::from_bytes::<blimp_ground_ws_interface::MessageG2V>(&msg_bin).unwrap();
                                    match msg_des {
                                        ms @ blimp_ground_ws_interface::MessageG2V::MotorSpeed { id, speed } => {
                                            println!("Updated speed: {:#?}", ms);
                                        }
                                        _ => {}
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    else {
                        println!("WebSocket connection closed!");
                        break;
                    }
                }
                _ = shutdown_rx.recv() => {
                    break;
                }
            };
            //tokio::task::yield_now().await;
        }
    });

    tokio::signal::ctrl_c().await.unwrap();
    println!("Shutting down...");
    shutdown_tx.send(()).unwrap();

    Ok(())
}
