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

const ELEVATION_CUMULATIVE: bool = false;

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
            let mut elevation_integral: f32 = 0.0;
            let mut flight_mode = FlightMode::Manual;
            let mut motors_toggles = [true; 4];
            let mut motors_reverse = [false; 4];
            let mut nav_lights = false;
            let mut last_control_time = std::time::Instant::now();

            async fn send_controls(
                ws_client: Arc<TMutex<BlimpGroundWebsocketClient>>,
                axes_values: &mut BTreeMap<BlimpSteeringAxis, f32>,
                elevation_integral: &mut f32,
                flight_mode: &mut FlightMode,
                motors_toggles: &mut [bool; 4],
                motors_reverse: &mut [bool; 4],
                nav_lights: &mut bool,
                last_control_time: &mut std::time::Instant,
            ) {
                let elevation = if ELEVATION_CUMULATIVE {
                    *elevation_integral += *axes_values
                        .get(&BlimpSteeringAxis::Elevation)
                        .unwrap_or(&0.0)
                        * 0.5
                        * ((std::time::Instant::now() - *last_control_time).as_micros() as f32
                            / 1000000.0);
                    *elevation_integral = elevation_integral.clamp(-1.0, 1.0);
                    *elevation_integral
                        + *axes_values
                            .get(&BlimpSteeringAxis::ElevationTrim)
                            .unwrap_or(&0.0)
                } else {
                    *axes_values
                        .get(&BlimpSteeringAxis::Elevation)
                        .unwrap_or(&0.0)
                        + *axes_values
                            .get(&BlimpSteeringAxis::ElevationTrim)
                            .unwrap_or(&0.0)
                };
                *last_control_time = std::time::Instant::now();

                ws_client
                    .lock()
                    .await
                    .send(MessageV2G::Controls(Controls {
                        throttle_main: *axes_values
                            .get(&BlimpSteeringAxis::Throttle)
                            .unwrap_or(&0.0),
                        throttle_split: [0, 1, 2, 3].map(|i| {
                            *axes_values
                                .get(&BlimpSteeringAxis::ThrottleSplit(i))
                                .unwrap_or(&0.0)
                        }),
                        sideways: *axes_values
                            .get(&BlimpSteeringAxis::Sideways)
                            .unwrap_or(&0.0),
                        elevation,
                        pitch: *axes_values.get(&BlimpSteeringAxis::Pitch).unwrap_or(&0.0),
                        roll: *axes_values.get(&BlimpSteeringAxis::Roll).unwrap_or(&0.0),
                        yaw: *axes_values.get(&BlimpSteeringAxis::Yaw).unwrap_or(&0.0),
                        desired_flight_mode: flight_mode.clone(),
                        motors_toggles: motors_toggles.clone(),
                        motors_reverse: motors_reverse.clone(),
                        nav_lights: *nav_lights,
                    }))
                    .await
                    .unwrap();
            }

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
                                        BlimpButtonFunction::NavLightsToggle => {
                                            if state {
                                                nav_lights = !nav_lights;
                                            }
                                        }
                                        BlimpButtonFunction::CameraCycle => {
                                            if state {
                                                // println!("Before cycling camera");
                                                ws_client
                                                    .lock()
                                                    .await
                                                    .send(MessageV2G::CycleCamera)
                                                    .await
                                                    .unwrap();
                                                // println!("After cycling camera");
                                            }
                                        }
                                    }
                                }
                            },
                            None => {
                                break;
                            }
                        }

                        if (std::time::Instant::now() - last_control_time).as_millis() >= 100 {
                            send_controls(
                                ws_client.clone(),
                                &mut axes_values,
                                &mut elevation_integral,
                                &mut flight_mode,
                                &mut motors_toggles,
                                &mut motors_reverse,
                                &mut nav_lights,
                                &mut last_control_time)
                            .await;
                        }
                    }
                    _ = tokio::time::sleep(tokio::time::Duration::from_millis(100)) => {
                        // We probably don't even have to check this condition
                        if (std::time::Instant::now() - last_control_time).as_millis() >= 100 {
                            send_controls(
                                ws_client.clone(),
                                &mut axes_values,
                                &mut elevation_integral,
                                &mut flight_mode,
                                &mut motors_toggles,
                                &mut motors_reverse,
                                &mut nav_lights,
                                &mut last_control_time)
                            .await;
                        }
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
