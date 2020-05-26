use sdl2::event::Event;
use std::time::{Duration, Instant};
use sdl2::pixels::Color;
use sdl2::rect::Rect;

fn main() -> Result<(), String> {
    let sdl_context = sdl2::init()?;
    let video_subsystem = sdl_context.video()?;
    let window = video_subsystem
        .window("Main Window", 1280, 720)
        .build()
        .map_err(|x| x.to_string())?;
    let mut canvas = window.into_canvas().present_vsync().build().map_err(|x| x.to_string())?;
    let mut event_pump = sdl_context.event_pump()?;

    let start_time = Instant::now();

    'main: loop {
        let elapsed = Instant::now() - start_time;
        canvas.set_draw_color(Color::WHITE);
        canvas.clear();
        canvas.set_draw_color(Color::RED);
        canvas.draw_rect(Rect::new(elapsed.as_millis() as i32 / 10, 100, 50, 50));
        canvas.present();

        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. } => {
                    break 'main;
                }
                _ => {}
            }
        }

        // std::thread::sleep(Duration::from_secs_f32(1.0 / 60.0));
    }

    Ok(())
}
