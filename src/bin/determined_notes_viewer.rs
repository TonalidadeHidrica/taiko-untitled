use std::{
    cmp::Reverse,
    collections::{binary_heap::PeekMut, BinaryHeap},
    path::PathBuf,
};

use anyhow::anyhow;
use clap::Parser;
use config::Config;

use fs_err::File;
use itertools::Itertools;
use ordered_float::NotNan;
use sdl2::{
    event::Event,
    keyboard::{Keycode, Scancode},
    pixels::Color,
    rect::Rect,
    render::{TextureCreator, WindowCanvas},
    ttf::Font,
    video::WindowContext,
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

        note_hit_x: (523_08700, 5),
        cursor_digit: 4,
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
                Event::KeyDown {
                    keycode: Some(keycode),
                    ..
                } => match keycode {
                    Keycode::K => app_state.note_hit_x.0 += 10u64.pow(app_state.cursor_digit as _),
                    Keycode::J => app_state.note_hit_x.0 -= 10u64.pow(app_state.cursor_digit as _),
                    Keycode::L => app_state.cursor_digit = (app_state.cursor_digit - 1).clamp(0, 8),
                    Keycode::H => app_state.cursor_digit = (app_state.cursor_digit + 1).clamp(0, 8),
                    _ => {}
                },
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

    note_hit_x: (u64, i32),
    cursor_digit: i32,
}
impl AppState {
    fn to_x(&self, time: f64) -> f64 {
        self.origin_x + time * self.scale_x
    }
    #[allow(unused)]
    fn x_to_time(&self, x: f64) -> f64 {
        (x - self.origin_x) / self.scale_x
    }

    fn note_hit_x(&self) -> f64 {
        let (a, b) = self.note_hit_x;
        a as f64 / 10.0f64.powi(b)
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

    let notes = data
        .determined
        .notes
        .iter()
        .map(|note| {
            (
                note,
                NotNan::new((app_state.note_hit_x() - note.b) / note.a).unwrap(),
            )
        })
        .sorted_by_key(|x| x.1)
        .collect_vec();

    for &(note, t) in &notes {
        let x = app_state.to_x(*t);
        if x + 100.0 < 0.0 || 2880.0 < x - 100.0 {
            continue;
        }
        let rect = Rect::from_center((x as i32, 200), 9, 9);
        canvas.set_draw_color(get_single_note_color(note.kind));
        canvas.fill_rect(rect)?;
    }

    {
        let ratio = 210.0 / (*notes[1].1 - *notes[0].1);
        let mut heap = BinaryHeap::<(Reverse<i32>, usize)>::new();
        for (&(_, s), &(_, t)) in notes.iter().tuple_windows() {
            let beat = (t - s) * ratio;
            let sx = app_state.to_x(*s);
            let tx = app_state.to_x(*t);
            if tx < 0.0 || 2880.0 < sx {
                continue;
            }
            let x = (sx + tx) as i32 / 2;
            let text_surface = font
                .render(&format!("{:.3}", beat))
                .solid(Color::YELLOW)
                .map_err(|e| e.to_string())?;
            let (w, h) = (text_surface.width(), text_surface.height());
            let text_texture = texture_creator
                .create_texture_from_surface(text_surface)
                .map_err(|e| e.to_string())?;
            let half_w = w as i32 / 2 + 5;
            let heap_len = heap.len();
            let slot = match heap.peek_mut() {
                None => 0,
                Some(p) if p.0 .0 <= x - half_w => PeekMut::pop(p).1,
                _ => heap_len,
            };
            heap.push((Reverse(x + half_w), slot));
            let y = 250 + 30 * slot as i32;
            let rect = Rect::new(sx as i32, y - 5, (tx - sx) as u32, 10);
            canvas.set_draw_color(Color::GRAY);
            canvas.draw_rect(rect)?;
            let rect = Rect::from_center((x, y), w, h);
            canvas.copy(&text_texture, None, rect)?;
        }
    }

    for &(_, (s, t)) in &data.determined.segments {
        let sx = app_state.to_x(s);
        let tx = app_state.to_x(t);
        let rect = Rect::new(sx as i32, 100, (tx - sx) as u32, 20);
        canvas.set_draw_color(Color::WHITE);
        canvas.draw_rect(rect)?;
    }

    {
        let text_surface = font
            .render(&format!("note_hit_x = {:.5}", app_state.note_hit_x()))
            .solid(Color::WHITE)
            .map_err(|e| e.to_string())?;
        let (w, h) = (text_surface.width(), text_surface.height());
        let text_texture = texture_creator
            .create_texture_from_surface(text_surface)
            .map_err(|e| e.to_string())?;
        let rect = Rect::new(0, 0, w, h);
        canvas.copy(&text_texture, None, rect)?;
    }

    canvas.present();
    Ok(())
}
