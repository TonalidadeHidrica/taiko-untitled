use config::Config;
use sdl2::event::Event;
use sdl2::image::LoadTexture;
use sdl2::pixels::Color;
use sdl2::rect::Rect;
use std::path::PathBuf;
use std::time::Instant;

fn main() -> Result<(), String> {
    let mut image_paths = Config::default();
    let image_paths = image_paths
        .merge(config::File::with_name("image_paths.toml"))
        .map_err(|x| x.to_string())?;
    let image_path = image_paths
        .get::<PathBuf>("img")
        .map_err(|x| x.to_string())?;

    let sdl_context = sdl2::init()?;
    let video_subsystem = sdl_context.video()?;
    let window = video_subsystem
        .window("Main Window", 1280, 720)
        .build()
        .map_err(|x| x.to_string())?;
    let mut canvas = window
        .into_canvas()
        .present_vsync()
        .build()
        .map_err(|x| x.to_string())?;
    let mut event_pump = sdl_context.event_pump()?;

    let texture_creator = canvas.texture_creator();
    let texture = texture_creator.load_texture(image_path)?;

    let start_time = Instant::now();

    'main: loop {
        let elapsed = Instant::now() - start_time;
        canvas.set_draw_color(Color::WHITE);
        canvas.clear();
        canvas.set_draw_color(Color::RED);
        for i in 0..10 {
            canvas.copy(
                &texture,
                Some(Rect::new(130, 0, 130, 130)),
                Rect::new(1280 - elapsed.as_millis() as i32 / 10 - i * 30, 100, 130, 130),
            )?;
        }
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
