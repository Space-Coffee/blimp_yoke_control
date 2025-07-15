use std::collections::BTreeMap;
use std::sync::Arc;

use tokio::sync::{broadcast, mpsc, Mutex as TMutex};

use crate::{
    config_file::{BlimpButtonFunction, BlimpSteeringAxis, ConfigFile},
    YokeEvent,
};
use blimp_ground_ws_interface::{
    BlimpGroundWebsocketClient, Controls, FlightMode, MessageV2G, VizInterest,
};

pub async fn ws_client_start(
    shutdown_tx: broadcast::Sender<()>,
    mut yoke_rx: mpsc::Receiver<YokeEvent>,
    config: Arc<ConfigFile>,
) {
    let ws_addr = &config.ws_addr;

    let mut ws_client = BlimpGroundWebsocketClient::new(ws_addr);
    ws_client
        .connect()
        .await
        .expect("Failed to connect to the WS server");
    println!("Opened WebSocket connection");

    ws_client
        .send(MessageV2G::DeclareInterest(VizInterest {
            motors: true,
            servos: false,
            sensors: false,
            state: false,
        }))
        .await
        .unwrap();

    let ws_client = Arc::new(TMutex::new(ws_client));

    {
        let mut shutdown_rx = shutdown_tx.subscribe();
        let ws_client = ws_client.clone();
        tokio::spawn(async move {
            let mut axes_values = BTreeMap::<BlimpSteeringAxis, f32>::new();
            let mut flight_mode = FlightMode::Manual;
            let mut motors_toggles = [true; 4];
            let mut motors_reverse = [false; 4];
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
                                        BlimpButtonFunction::MotorToggle(motor) => {
                                            if state {
                                                motors_toggles[motor as usize] = !motors_toggles[motor as usize];
                                            }
                                        }
                                        BlimpButtonFunction::MotorReverse(motor) => {
                                            if state {
                                                motors_reverse[motor as usize] = !motors_reverse[motor as usize];
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
                            .send(MessageV2G::Controls(
                                Controls {
                                    throttle_main: *axes_values
                                        .get(&BlimpSteeringAxis::Throttle)
                                        .unwrap_or(&0.0),
                                    throttle_split: [0, 1, 2, 3]
                                        .map(|i| *axes_values
                                            .get(&BlimpSteeringAxis::ThrottleSplit(i))
                                            .unwrap_or(&0.0)),
                                    sideways: *axes_values
                                        .get(&BlimpSteeringAxis::Sideways)
                                        .unwrap_or(&0.0),
                                    elevation: *axes_values
                                        .get(&BlimpSteeringAxis::Elevation)
                                        .unwrap_or(&0.0),
                                    pitch: *axes_values
                                        .get(&BlimpSteeringAxis::Pitch)
                                        .unwrap_or(&0.0),
                                    roll: *axes_values
                                        .get(&BlimpSteeringAxis::Roll)
                                        .unwrap_or(&0.0),
                                    yaw: *axes_values
                                        .get(&BlimpSteeringAxis::Yaw)
                                        .unwrap_or(&0.0),
                                    desired_flight_mode: flight_mode.clone(),
                                    motors_toggles: motors_toggles.clone(),
                                    motors_reverse: motors_reverse.clone(),
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
