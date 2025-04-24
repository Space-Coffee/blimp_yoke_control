use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};

use crate::{AxesMapping, BlimpSteeringAxis, YokeEvent};

pub async fn ws_client_start(
    shutdown_tx: tokio::sync::broadcast::Sender<()>,
    mut yoke_rx: tokio::sync::mpsc::Receiver<YokeEvent>,
    mapping: Arc<AxesMapping>,
) {
    //TODO: Allow configuring WS address
    let ws_addr = "ws://127.0.0.1:8765";

    let mut ws_client = blimp_ground_ws_interface::BlimpGroundWebsocketClient::new(ws_addr);
    ws_client
        .connect()
        .await
        .expect("Failed to connect to the WS server");
    println!("Opened WebSocket connection");

    ws_client
        .send(blimp_ground_ws_interface::MessageV2G::DeclareInterest(
            blimp_ground_ws_interface::VizInterest {
                motors: true,
                servos: false,
                sensors: false,
            },
        ))
        .await
        .unwrap();

    {
        let mut shutdown_rx = shutdown_tx.subscribe();
        //let ws_stream = ws_stream.clone();
        tokio::spawn(async move {
            let mut axes_values = std::collections::BTreeMap::<BlimpSteeringAxis, i32>::new();
            loop {
                tokio::select! {
                    yoke_ev = yoke_rx.recv() => {
                        //println!("{:?}", yoke_ev);
                        match yoke_ev {
                            Some(crate::YokeEvent::AxisMotion {joy_id, axis, value }) => {
                                if let Some(mapped_axis) = mapping.joys[joy_id as usize].axes.get(&axis){
                                    axes_values.insert(
                                        mapped_axis.0.clone(),
                                        (
                                            ((value as i64) - ((mapped_axis.1 as i64) + (mapped_axis.2 as i64)) / 2) *
                                            0xFFFF / ((mapped_axis.2 as i64)-(mapped_axis.1 as i64))
                                        ).try_into().unwrap()
                                    );
                                }
                            },
                            Some(crate::YokeEvent::ButtonState { joy_id: _, button: _, state: _ }) => {},
                            None => {
                                break;
                            }
                        }

                        ws_client.send(blimp_ground_ws_interface::MessageV2G::Controls(
                            blimp_ground_ws_interface::Controls {
                                throttle: *axes_values.get(&crate::BlimpSteeringAxis::Throttle).unwrap_or(&0),
                                elevation: *axes_values.get(&crate::BlimpSteeringAxis::Elevation).unwrap_or(&0),
                                yaw: *axes_values.get(&crate::BlimpSteeringAxis::Yaw).unwrap_or(&0),
                            },
                        )).await.unwrap();
                    }
                    _ = shutdown_rx.recv() => {
                        break;
                    }
                }
                //tokio::task::yield_now().await;
            }
        });
    }

    {
        let shutdown_tx = shutdown_tx.clone();
        let mut shutdown_rx = shutdown_tx.subscribe();
        tokio::spawn(async move {
            loop {
                //let mut ws_stream_locked = ws_stream.lock().await;
                tokio::select! {
                    ws_msg = ws_client.recv() => {
                        //println!("Received some WS message: {:#?}", ws_msg);
                        if let Ok(ws_msg) = ws_msg{
                            //println!("Got WS G2V message: {:#?}", ws_msg);
                            match ws_msg {
                                ms @ blimp_ground_ws_interface::MessageG2V::MotorSpeed { id, speed } => {
                                    // println!("Updated speed: {:#?}", ms);
                                }
                                _ => {}
                            }
                        }
                        else {
                            println!("WebSocket connection closed!");
                            shutdown_tx.send(()).unwrap();
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
    }
}
