use futures_util::{SinkExt, StreamExt};

pub async fn ws_client_start(
    shutdown_tx: tokio::sync::broadcast::Sender<()>,
    mut yoke_rx: tokio::sync::mpsc::Receiver<crate::YokeEvent>,
    axes_mapping: std::collections::BTreeMap<u8, crate::AxesMappingEntry>,
) {
    //TODO: Allow configuring WS address
    let ws_addr = "ws://127.0.0.1:8765";

    let (mut ws_stream, _) = tokio_tungstenite::connect_async(
        tokio_tungstenite::tungstenite::client::IntoClientRequest::into_client_request(ws_addr)
            .expect(format!("Malformed WebSocket address: {}", ws_addr).as_str()),
    )
    .await
    .expect(format!("Couldn't open WebSocket connection with {}", ws_addr).as_str());
    println!("Opened WebSocket connection");

    //let ws_stream = std::sync::Arc::new(tokio::sync::Mutex::new(ws_stream));

    let (mut ws_stream_tx, mut ws_stream_rx) = ws_stream.split();

    ws_stream_tx
        .send(tokio_tungstenite::tungstenite::Message::Binary(
            postcard::to_stdvec::<blimp_ground_ws_interface::MessageV2G>(
                &blimp_ground_ws_interface::MessageV2G::DeclareInterest(
                    blimp_ground_ws_interface::VizInterest {
                        motors: true,
                        servos: false,
                        sensors: false,
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
            let mut axes_values =
                std::collections::BTreeMap::<crate::BlimpSteeringAxis, i32>::new();
            loop {
                tokio::select! {
                    yoke_ev = yoke_rx.recv() => {
                        //println!("{:?}", yoke_ev);
                        match yoke_ev {
                            Some(crate::YokeEvent::AxisMotion { axis, value }) => {
                                if let Some(mapped_axis) = axes_mapping.get(&axis){
                                    axes_values.insert(
                                        mapped_axis.0.clone(),
                                        (
                                            ((value as i64) - ((mapped_axis.1 as i64) + (mapped_axis.2 as i64)) / 2) *
                                            0xFFFF / ((mapped_axis.2 as i64)-(mapped_axis.1 as i64))
                                        ).try_into().unwrap()
                                    );
                                }
                            },
                            Some(crate::YokeEvent::ButtonState { button, state }) => {},
                            None => {
                                break;
                            }
                        }
                        let msg_ser = postcard::to_stdvec::<blimp_ground_ws_interface::MessageV2G>(
                                    &blimp_ground_ws_interface::MessageV2G::Controls(
                                        blimp_ground_ws_interface::Controls {
                                            throttle: *axes_values.get(&crate::BlimpSteeringAxis::Throttle).unwrap_or(&0),
                                            elevation: *axes_values.get(&crate::BlimpSteeringAxis::Elevation).unwrap_or(&0),
                                            yaw: *axes_values.get(&crate::BlimpSteeringAxis::Yaw).unwrap_or(&0),
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

    {
        let shutdown_tx = shutdown_tx.clone();
        let mut shutdown_rx = shutdown_tx.subscribe();
        tokio::spawn(async move {
            loop {
                //let mut ws_stream_locked = ws_stream.lock().await;
                tokio::select! {
                    ws_msg = ws_stream_rx.next() => {
                        //println!("Received some WS message: {:#?}", ws_msg);
                        if let Some(ws_msg) = ws_msg{
                            if let Ok(ws_msg) = ws_msg{
                                //println!("Got WS G2V message: {:#?}", ws_msg);
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
