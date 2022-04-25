use std::path::PathBuf;

use anyhow::anyhow;
use clap::Parser;
use fs_err::File;
use sdl2::{event::Event, pixels::Color, rect::Rect, render::WindowCanvas};
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
    println!("{:?}, {:?}", window.size(), window.drawable_size());
    let mut canvas = window
        .into_canvas()
        .present_vsync()
        .build()
        .map_err(|e| anyhow!("{}", e))?;
    let mut event_pump = sdl_context.event_pump().map_err(|e| anyhow!("{}", e))?;
    let mouse_util = sdl_context.mouse();
    let ttf_context = sdl2::ttf::init().map_err(|e| anyhow!("{}", e))?;

    let mut app_state = AppState { origin_y: 40 };

    'main: loop {
        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. } => break 'main,
                Event::MouseWheel { y, .. } => {
                    app_state.origin_y += y * 10;
                }
                _ => {}
            }
        }

        draw(&mut canvas, &data, &app_state).map_err(|e| anyhow!("{}", e))?;
    }

    Ok(())
}

struct AppState {
    origin_y: i32,
}

fn draw(
    canvas: &mut WindowCanvas,
    data: &NotePositionsResult,
    app_state: &AppState,
) -> Result<(), String> {
    canvas.set_draw_color(Color::BLACK);
    canvas.clear();

    for (&pts, frame) in &data.results {
        let y = app_state.origin_y + (pts / 64) as i32;
        for note in &frame.notes {
            let x = note.note_x() as i32;
            let rect = Rect::from_center((x, y), 3, 3);
            canvas.set_draw_color(get_single_note_color(note.kind));
            canvas.fill_rect(rect)?;
        }
    }

    canvas.present();
    Ok(())
}
