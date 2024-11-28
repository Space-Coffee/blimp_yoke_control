pub fn sdl_thread(
    yoke_tx: tokio::sync::mpsc::Sender<crate::YokeEvent>,
    shutdown_tx: tokio::sync::broadcast::Sender<()>,
) {
    let mut shutdown_rx = shutdown_tx.subscribe();

    let sdl_ctx = sdl2::init().unwrap();
    let mut event_pump = sdl_ctx.event_pump().unwrap();
    let joystick_subsys = sdl_ctx.joystick().unwrap();
    let joystick = match joystick_subsys.open(0) {
        Ok(js) => js,
        Err(err) => {
            shutdown_tx.send(()).unwrap();
            panic!("Couldn't open joystick");
        }
    };

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
                        .blocking_send(crate::YokeEvent::AxisMotion {
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
                        .blocking_send(crate::YokeEvent::ButtonState {
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
                        .blocking_send(crate::YokeEvent::ButtonState {
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
