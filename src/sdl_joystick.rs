use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use regex;

use crate::{AxesMapping, YokeEvent};

pub fn sdl_thread(
    yoke_tx: tokio::sync::mpsc::Sender<YokeEvent>,
    shutdown_tx: tokio::sync::broadcast::Sender<()>,
    axes_mapping: Arc<AxesMapping>,
) {
    let mut shutdown_rx = shutdown_tx.subscribe();

    let sdl_ctx = sdl2::init().expect("Couldn't initialize SDL2");
    let mut event_pump = sdl_ctx
        .event_pump()
        .expect("Couldn't initialize SDL2 event pump");
    let joystick_subsys = sdl_ctx
        .joystick()
        .expect("Couldn't initialize SDL2 joystick subsystem");

    let joys_count = joystick_subsys.num_joysticks().unwrap();
    println!("Joysticks count: {}", joys_count);
    for i in 0..joys_count {
        println!("{}: {}", i, joystick_subsys.name_for_index(i).unwrap());
    }

    // TODO: Allow selecting joystick
    let mut joys_instances = Vec::<sdl2::joystick::Joystick>::new();
    let mut used_joys_ids_mappings = BTreeMap::<u32, u32>::new();
    for (joy_sym_id, joy) in axes_mapping.joys.iter().enumerate() {
        let name_regex = regex::Regex::new(&joy.name_regex).unwrap();
        let mut joystick_id: Option<u32> = None;
        for i in 0..joys_count {
            if used_joys_ids_mappings.contains_key(&i) {
                continue;
            }
            if name_regex.is_match(&joystick_subsys.name_for_index(i).unwrap()) {
                used_joys_ids_mappings.insert(i, joy_sym_id as u32);
                joystick_id = Some(i);
                break;
            }
        }
        if let Some(joystick_id) = joystick_id {
            let joystick = match joystick_subsys.open(joystick_id) {
                Ok(js) => js,
                Err(err) => {
                    shutdown_tx.send(()).unwrap();
                    panic!("Couldn't open joystick with id {joystick_id}");
                }
            };
            joys_instances.push(joystick);
        } else {
            panic!("Matching joystick not found!");
        }
    }

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
                            joy_id: *used_joys_ids_mappings
                                .get(&which)
                                .expect("Received event from unknown joystick"),
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
                            joy_id: *used_joys_ids_mappings
                                .get(&which)
                                .expect("Received event from unknown joystick"),
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
                            joy_id: 0,
                            button: button_idx,
                            state: false,
                        })
                        .unwrap();
                }
                _ => {}
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}
