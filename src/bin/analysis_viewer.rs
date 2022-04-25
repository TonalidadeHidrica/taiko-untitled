use std::path::PathBuf;

use anyhow::anyhow;
use clap::Parser;
use sdl2::event::Event;

#[derive(Parser)]
struct Opts {
    json_path: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();

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

    'main: loop {
        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. } => break 'main,
                _ => {}
            }
        }
    }

    Ok(())
}
