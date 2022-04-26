use std::path::PathBuf;

use anyhow::anyhow;
use clap::Parser;
use fs_err::File;
use sdl2::{event::Event, keyboard::Scancode, pixels::Color, rect::Rect, render::WindowCanvas};
use taiko_untitled::{analyze::NotePositionsResult, video_analyzer_assets::get_single_note_color};

#[derive(Parser)]
struct Opts {
    json_path: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();

    let data: NotePositionsResult = serde_json::from_reader(File::open(&opts.json_path)?)?;

    let width = 960;
    let height = 540;

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
    let mut event_pump = sdl_context.event_pump().map_err(|e| anyhow!("{}", e))?;
    let mouse_util = sdl_context.mouse();
    let ttf_context = sdl2::ttf::init().map_err(|e| anyhow!("{}", e))?;

    let dpi_factor = canvas.window().drawable_size().0 as f64 / canvas.window().size().0 as f64;

    let mut app_state = AppState {
        origin_y: 40.0,
        scale_y: 1.0 / 64.0,
    };

    'main: loop {
        let keyboard_state = event_pump.keyboard_state();
        let shift = keyboard_state.is_scancode_pressed(Scancode::LShift)
            || keyboard_state.is_scancode_pressed(Scancode::RShift);
        let alt = keyboard_state.is_scancode_pressed(Scancode::LAlt)
            || keyboard_state.is_scancode_pressed(Scancode::RAlt);
        let mouse_state = event_pump.mouse_state();
        let mouse_x = mouse_state.x() as f64 * dpi_factor;
        let mouse_y = mouse_state.y() as f64 * dpi_factor;
        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. } => break 'main,
                Event::MouseWheel { y, .. } => {
                    let y = y as f64;
                    if shift {
                        let scale_factor = 1.05f64.powf(-y);
                        app_state.origin_y = mouse_y + (app_state.origin_y - mouse_y) * scale_factor;
                        app_state.scale_y *= scale_factor;
                    } else {
                        app_state.origin_y += y * 10.0;
                    }
                }
                _ => {}
            }
        }

        draw(&mut canvas, &data, &app_state).map_err(|e| anyhow!("{}", e))?;
    }

    Ok(())
}

struct AppState {
    origin_y: f64,
    scale_y: f64,
}

fn draw(
    canvas: &mut WindowCanvas,
    data: &NotePositionsResult,
    app_state: &AppState,
) -> Result<(), String> {
    canvas.set_draw_color(Color::BLACK);
    canvas.clear();

    for (&pts, frame) in &data.results {
        let y = app_state.origin_y + pts as f64 * app_state.scale_y;
        for note in &frame.notes {
            let x = note.note_x() as i32;
            let rect = Rect::from_center((x, y as i32), 3, 3);
            canvas.set_draw_color(get_single_note_color(note.kind));
            canvas.fill_rect(rect)?;
        }
    }

    canvas.present();
    Ok(())
}
