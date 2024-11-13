use sdl2;
use tokio;

#[derive(Debug)]
enum YokeEvent {
    AxisMotion { axis: u8, value: i16 },
    ButtonState { button: u8, state: bool },
}

fn sdl_thread(yoke_tx: tokio::sync::mpsc::Sender<YokeEvent>) {
    let sdl_ctx = sdl2::init().unwrap();
    let mut event_pump = sdl_ctx.event_pump().unwrap();
    let joystick_subsys = sdl_ctx.joystick().unwrap();
    let joystick = joystick_subsys.open(0).expect("Couldn't open joystick");

    'ev_loop: loop {
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
async fn main() {
    println!("Hello, world!");

    let (yoke_tx, mut yoke_rx) = tokio::sync::mpsc::channel::<YokeEvent>(128);

    std::thread::spawn(move || {
        sdl_thread(yoke_tx);
    });

    while let Some(yoke_ev) = yoke_rx.recv().await {
        println!("{:?}", yoke_ev);
    }
}
