use std::time::Duration;
use sdl2::event::Event;

fn main() -> Result<(), String> {
    let sdl_context = sdl2::init()?;
    let video_subsystem = sdl_context.video()?;
    let window = video_subsystem.window("Main Window", 1280, 720)
        .build().map_err(|x| x.to_string())?;
    let mut event_pump = sdl_context.event_pump()?;

    'main: loop {
        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. } => {
                    println!("Quitting.");
                    break 'main;
                }
                _ => {}
            }
        }

        std::thread::sleep(Duration::from_secs_f32(1.0 / 60.0));
    }

    Ok(())
}
