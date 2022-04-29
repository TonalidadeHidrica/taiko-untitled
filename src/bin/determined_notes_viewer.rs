use std::path::PathBuf;

use anyhow::anyhow;
use clap::Parser;
use config::Config;

use fs_err::File;
use sdl2::{
    event::Event,
    pixels::Color,
    rect::Rect,
    render::{TextureCreator, WindowCanvas},
    ttf::Font,
    video::WindowContext, keyboard::Scancode,
};
use taiko_untitled::{
    analyze::DetermineFrameTimeResult, sdl2_utils::enable_momentum_scroll,
    video_analyzer_assets::get_single_note_color,
};

#[derive(Parser)]
struct Opts {
    determined_path: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    let mut config = Config::default();
    let config = config.merge(config::File::with_name("config.toml"))?;

    enable_momentum_scroll();

    let data = AppData {
        determined: serde_json::from_reader(File::open(&opts.determined_path)?)?,
    };

    let width = 1440;
    let height = 810;

    let sdl_context = sdl2::init().map_err(|e| anyhow!("{}", e))?;
    let video_subsystem = sdl_context.video().map_err(|e| anyhow!("{}", e))?;
    let window = video_subsystem
        .window("Main Window", width, height)
        .allow_highdpi()
        .build()
        .map_err(|e| anyhow!("{}", e))?;
    let mut canvas = window
        .into_canvas()
        .present_vsync()
        .build()
        .map_err(|e| anyhow!("{}", e))?;
    let texture_creator = canvas.texture_creator();
    let mut event_pump = sdl_context.event_pump().map_err(|e| anyhow!("{}", e))?;
    let ttf_context = sdl2::ttf::init().map_err(|e| anyhow!("{}", e))?;
    let font = ttf_context
        .load_font(&config.get::<PathBuf>("font")?, 32)
        .map_err(|e| anyhow!("{}", e))?;

    let dpi_factor = canvas.window().drawable_size().0 as f64 / canvas.window().size().0 as f64;

    let mut app_state = AppState {
        origin_x: 0.0,
        scale_x: 0.05,

        note_hit_x: 522.5,
    };

    'main: loop {
        let keyboard_state = event_pump.keyboard_state();
        let shift = keyboard_state.is_scancode_pressed(Scancode::LShift)
            || keyboard_state.is_scancode_pressed(Scancode::RShift);
        let mouse_state = event_pump.mouse_state();
        let mouse_x = mouse_state.x() as f64 * dpi_factor;
        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. } => break 'main,
                Event::MouseWheel { x, y, .. } => {
                    let x = x as f64;
                    let y = y as f64;
                    if shift {
                        let scale_factor = 1.05f64.powf(-y);
                        app_state.origin_x =
                            mouse_x + (app_state.origin_x - mouse_x) * scale_factor;
                        app_state.scale_x *= scale_factor;
                    } else {
                        app_state.origin_x -= x * 10.0;
                    }
                }
                _ => {}
            }
        }

        draw(&mut canvas, &texture_creator, &font, &data, &app_state)
            .map_err(|e| anyhow!("{}", e))?;
    }

    Ok(())
}

struct AppData {
    determined: DetermineFrameTimeResult,
}

struct AppState {
    origin_x: f64,
    scale_x: f64,

    note_hit_x: f64,
}
impl AppState {
    fn to_x(&self, time: f64) -> f64 {
        self.origin_x + time * self.scale_x
    }
    fn x_to_time(&self, x: f64) -> f64 {
        (x - self.origin_x) / self.scale_x
    }
}

fn draw(
    canvas: &mut WindowCanvas,
    texture_creator: &TextureCreator<WindowContext>,
    font: &Font,
    data: &AppData,
    app_state: &AppState,
) -> Result<(), String> {
    canvas.set_draw_color(Color::BLACK);
    canvas.clear();

    for note in &data.determined.notes {
        let x = app_state.to_x((app_state.note_hit_x - note.b) / note.a);
        if x + 100.0 < 0.0 || 2880.0 < x - 100.0 {
            continue;
        }
        let rect = Rect::from_center((x as i32, 200), 9, 9);
        canvas.set_draw_color(get_single_note_color(note.kind));
        canvas.fill_rect(rect)?;
    }
    for &(_, (s, t)) in &data.determined.segments {
        let sx = app_state.to_x(s);
        let tx = app_state.to_x(t);
        let rect = Rect::new(sx as i32, 100, (tx - sx) as u32, 20);
        canvas.set_draw_color(Color::WHITE);
        canvas.draw_rect(rect)?;
    }

    canvas.present();
    Ok(())
}
