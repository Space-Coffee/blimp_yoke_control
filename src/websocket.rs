use std::sync::Arc;

use tokio::sync::Mutex as TMutex;

use crate::{
    config_file::{BlimpButtonFunction, BlimpSteeringAxis, ConfigFile},
    YokeEvent,
};
use blimp_ground_ws_interface::FlightMode;

pub async fn ws_client_start(
    shutdown_tx: tokio::sync::broadcast::Sender<()>,
    mut yoke_rx: tokio::sync::mpsc::Receiver<YokeEvent>,
    config: Arc<ConfigFile>,
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
                state: false,
            },
        ))
        .await
        .unwrap();

    let ws_client = Arc::new(TMutex::new(ws_client));

    {
        let mut shutdown_rx = shutdown_tx.subscribe();
        let ws_client = ws_client.clone();
        tokio::spawn(async move {
            let mut axes_values = std::collections::BTreeMap::<BlimpSteeringAxis, f32>::new();
            let mut flight_mode = FlightMode::Manual;
            loop {
                tokio::select! {
                    yoke_ev = yoke_rx.recv() => {
                        //println!("{:?}", yoke_ev);
                        match yoke_ev {
                            Some(YokeEvent::AxisMotion {joy_id, axis, value }) => {
                                if let Some(mapped_axis) = config.joys[joy_id as usize].axes.get(&axis) {
                                    let mut keypoint_num = 0;
                                    while keypoint_num < mapped_axis.keypoints.len() {
                                        if mapped_axis.keypoints[keypoint_num + 1].0 >= value {
                                            break;
                                        }
                                        keypoint_num += 1;
                                    }
                                    // Invariant here: mapped_axis.keypoints[keypoint_num].0 <=
                                    // value <= mapped_axis.keypoints[keypoint_num + 1].0
                                    axes_values.insert(
                                        mapped_axis.axis.clone(),
                                        (value as f32 - mapped_axis.keypoints[keypoint_num].0 as f32)
                                            / (mapped_axis.keypoints[keypoint_num + 1].0 as f32 - mapped_axis.keypoints[keypoint_num].0 as f32)
                                            * (mapped_axis.keypoints[keypoint_num + 1].1 - mapped_axis.keypoints[keypoint_num].1)
                                            + mapped_axis.keypoints[keypoint_num].1
                                    );
                                }
                            },
                            Some(YokeEvent::ButtonState { joy_id, button, state }) => {
                                if let Some(mapped_button) = config.joys[joy_id as usize].buttons.get(&button) {
                                    match mapped_button.function {
                                        BlimpButtonFunction::FlightModeCycle => {
                                            if state {
                                                flight_mode = match flight_mode.clone() {
                                                    FlightMode::Manual => FlightMode::Atti,
                                                    FlightMode::Atti => FlightMode::AltiAtti,
                                                    FlightMode::AltiAtti => FlightMode::Manual,
                                                }
                                            }
                                        }
                                    }
                                }
                            },
                            None => {
                                break;
                            }
                        }

                        ws_client
                            .lock()
                            .await
                            .send(blimp_ground_ws_interface::MessageV2G::Controls(
                                blimp_ground_ws_interface::Controls {
                                    throttle_main: *axes_values
                                        .get(&BlimpSteeringAxis::Throttle)
                                        .unwrap_or(&0.0),
                                    elevation: *axes_values
                                        .get(&BlimpSteeringAxis::Elevation)
                                        .unwrap_or(&0.0),
                                    yaw: *axes_values
                                        .get(&BlimpSteeringAxis::Yaw)
                                        .unwrap_or(&0.0),
                                    throttle_split: [0.0; 4],
                                    sideways: 0.0,
                                    pitch: 0.0,
                                    roll: 0.0,
                                    desired_flight_mode: flight_mode.clone(),
                                },
                            ))
                            .await
                            .unwrap();
                    }
                    _ = shutdown_rx.recv() => {
                        break;
                    }
                }
                //tokio::task::yield_now().await;
            }
        });
    }
}
