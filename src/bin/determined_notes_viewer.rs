use std::{
    cmp::Reverse,
    collections::{binary_heap::PeekMut, BTreeMap, BinaryHeap},
    fmt::Debug,
    path::PathBuf,
};

use anyhow::anyhow;
use clap::Parser;
use config::Config;

use fs_err::File;
use itertools::Itertools;
use num::Integer;
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
    analyze::{
        make_cumulative_map, map_float, DetermineFrameTimeResult, DeterminedNote,
        VideoIntegralResult,
    },
    sdl2_utils::enable_momentum_scroll,
    video_analyzer_assets::get_single_note_color,
};

#[derive(Parser)]
struct Opts {
    determined_path: PathBuf,
    #[clap(long = "integrals")]
    integrals_path: Option<PathBuf>,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    let mut config = Config::default();
    let config = config.merge(config::File::with_name("config.toml"))?;

    enable_momentum_scroll();

    let determined: DetermineFrameTimeResult =
        serde_json::from_reader(File::open(&opts.determined_path)?)?;
    let pts_to_time = make_cumulative_map(determined.durations.iter().map(|(x, y)| (x, y)));
    let data = AppData {
        determined,
        pts_to_time,
        integrals: opts
            .integrals_path
            .as_ref()
            .map(|p| anyhow::Ok(serde_json::from_reader(File::open(p)?)?))
            .transpose()?,
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

    #[allow(clippy::zero_prefixed_literal)]
    let mut app_state = AppState {
        origin_x: 0.0,
        scale_x: 0.05,

        jump_combo: 0,

        note_hit_x: PreciseDecimal(523_08700, 5),
        speed_factor: PreciseDecimal(647_866, 3),
        speed_error_rate: PreciseDecimal(0_010_000, 6),
        duration_error_rate: PreciseDecimal(0_100_000, 6),
        cursor_digit: 4,
        cursor_num: 0,

        mouse_x: 0.0,
        drag_start_x: None,
    };

    'main: loop {
        let notes = data
            .determined
            .notes
            .iter()
            .map(|note| {
                (
                    note,
                    NotNan::new((app_state.note_hit_x.value() - note.b) / note.a).unwrap(),
                )
            })
            .sorted_by_key(|x| x.1)
            .collect_vec();

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
                Event::MouseMotion { x, .. } => app_state.mouse_x = x as f64 * dpi_factor,
                Event::MouseButtonDown { x, .. } => {
                    app_state.drag_start_x = Some(x as f64 * dpi_factor)
                }
                Event::MouseButtonUp { .. } => app_state.drag_start_x = None,
                Event::KeyDown {
                    keycode: Some(keycode),
                    ..
                } => match keycode {
                    #[allow(clippy::identity_op)]
                    Keycode::Num0 => {
                        app_state.jump_combo =
                            (|| app_state.jump_combo.checked_mul(10)?.checked_add(0))()
                                .unwrap_or(app_state.jump_combo)
                    }
                    Keycode::Num1 => {
                        app_state.jump_combo =
                            (|| app_state.jump_combo.checked_mul(10)?.checked_add(1))()
                                .unwrap_or(app_state.jump_combo)
                    }
                    Keycode::Num2 => {
                        app_state.jump_combo =
                            (|| app_state.jump_combo.checked_mul(10)?.checked_add(2))()
                                .unwrap_or(app_state.jump_combo)
                    }
                    Keycode::Num3 => {
                        app_state.jump_combo =
                            (|| app_state.jump_combo.checked_mul(10)?.checked_add(3))()
                                .unwrap_or(app_state.jump_combo)
                    }
                    Keycode::Num4 => {
                        app_state.jump_combo =
                            (|| app_state.jump_combo.checked_mul(10)?.checked_add(4))()
                                .unwrap_or(app_state.jump_combo)
                    }
                    Keycode::Num5 => {
                        app_state.jump_combo =
                            (|| app_state.jump_combo.checked_mul(10)?.checked_add(5))()
                                .unwrap_or(app_state.jump_combo)
                    }
                    Keycode::Num6 => {
                        app_state.jump_combo =
                            (|| app_state.jump_combo.checked_mul(10)?.checked_add(6))()
                                .unwrap_or(app_state.jump_combo)
                    }
                    Keycode::Num7 => {
                        app_state.jump_combo =
                            (|| app_state.jump_combo.checked_mul(10)?.checked_add(7))()
                                .unwrap_or(app_state.jump_combo)
                    }
                    Keycode::Num8 => {
                        app_state.jump_combo =
                            (|| app_state.jump_combo.checked_mul(10)?.checked_add(8))()
                                .unwrap_or(app_state.jump_combo)
                    }
                    Keycode::Num9 => {
                        app_state.jump_combo =
                            (|| app_state.jump_combo.checked_mul(10)?.checked_add(9))()
                                .unwrap_or(app_state.jump_combo)
                    }
                    Keycode::Escape => app_state.jump_combo = 0,
                    Keycode::Return => {
                        if (1..=notes.len()).contains(&app_state.jump_combo) {
                            let note = &notes[app_state.jump_combo - 1];
                            app_state.origin_x += width as f64 - app_state.to_x(*note.1);
                        }
                        app_state.jump_combo = 0;
                    }
                    Keycode::K | Keycode::J => {
                        if shift {
                            app_state.cursor_num = if keycode == Keycode::K {
                                app_state.cursor_num.saturating_sub(1)
                            } else {
                                app_state.cursor_num.saturating_add(1)
                            };
                            app_state.cursor_num = app_state.cursor_num.clamp(0, 3);
                        } else {
                            let pointer = match app_state.cursor_num {
                                0 => &mut app_state.note_hit_x.0,
                                1 => &mut app_state.speed_factor.0,
                                2 => &mut app_state.speed_error_rate.0,
                                _ => &mut app_state.duration_error_rate.0,
                            };
                            if keycode == Keycode::K {
                                *pointer += 10i64.pow(app_state.cursor_digit as _)
                            } else {
                                *pointer -= 10i64.pow(app_state.cursor_digit as _)
                            };
                        }
                    }
                    Keycode::L => {
                        app_state.cursor_digit = (app_state.cursor_digit - 1).clamp(0, 12)
                    }
                    Keycode::H => {
                        app_state.cursor_digit = (app_state.cursor_digit + 1).clamp(0, 12)
                    }
                    _ => {}
                },
                _ => {}
            }
        }

        draw(
            &mut canvas,
            &texture_creator,
            &font,
            &data,
            &app_state,
            &notes,
        )
        .map_err(|e| anyhow!("{}", e))?;
    }

    Ok(())
}

struct AppData {
    determined: DetermineFrameTimeResult,
    integrals: Option<VideoIntegralResult>,
    pts_to_time: BTreeMap<i64, f64>,
}

struct AppState {
    origin_x: f64,
    scale_x: f64,

    jump_combo: usize,

    note_hit_x: PreciseDecimal,
    speed_factor: PreciseDecimal,
    speed_error_rate: PreciseDecimal,
    duration_error_rate: PreciseDecimal,
    cursor_digit: i32,
    cursor_num: usize,

    mouse_x: f64,
    drag_start_x: Option<f64>,
}
impl AppState {
    fn to_x(&self, time: f64) -> f64 {
        self.origin_x + time * self.scale_x
    }
    #[allow(unused)]
    fn x_to_time(&self, x: f64) -> f64 {
        (x - self.origin_x) / self.scale_x
    }
}
#[derive(Clone, Copy)]
struct PreciseDecimal(i64, i32);
impl PreciseDecimal {
    fn value(self) -> f64 {
        self.0 as f64 / 10.0f64.powi(self.1)
    }
}
impl Debug for PreciseDecimal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (a, b) = self.0.abs().div_rem(&10i64.pow(self.1 as _));
        if self.0 < 0 {
            f.write_str("-")?;
        }
        f.write_fmt(format_args!("{}.{:0width$}", a, b, width = self.1 as _))
    }
}

fn draw(
    canvas: &mut WindowCanvas,
    texture_creator: &TextureCreator<WindowContext>,
    font: &Font,
    data: &AppData,
    app_state: &AppState,
    notes: &[(&DeterminedNote, NotNan<f64>)],
) -> Result<(), String> {
    canvas.set_draw_color(Color::BLACK);
    canvas.clear();

    for (&(note, t), i) in notes.iter().zip(1..) {
        let x = app_state.to_x(*t);
        if x + 100.0 < 0.0 || 2880.0 < x - 100.0 {
            continue;
        }
        let rect = Rect::from_center((x as i32, 200), 9, 9);
        canvas.set_draw_color(get_single_note_color(note.kind));
        canvas.fill_rect(rect)?;

        let speed = -note.a * app_state.speed_factor.value();
        draw_range_text(
            canvas,
            texture_creator,
            font,
            speed,
            app_state.speed_error_rate,
            (x as i32, 270),
            Color::GREEN,
            3,
        )?;

        let text_surface = font
            .render(&i.to_string())
            .solid(Color::WHITE)
            .map_err(|e| e.to_string())?;
        let (w, h) = (text_surface.width(), text_surface.height());
        let text_texture = texture_creator
            .create_texture_from_surface(text_surface)
            .map_err(|e| e.to_string())?;
        let rect = Rect::from_center((x as i32, 175), w, h);
        canvas.copy(&text_texture, None, rect)?;
    }

    {
        let ratio = 210.0 / (*notes[1].1 - *notes[0].1);
        let mut heap = BinaryHeap::<(Reverse<i32>, usize)>::new();
        let mut heap2 = BinaryHeap::<Reverse<usize>>::new();
        let notes = notes.iter().tuple_windows().map(|(s, t)| {
            (
                app_state.to_x(*s.1),
                app_state.to_x(*t.1),
                *(t.1 - s.1) * ratio,
            )
        });
        let drag = app_state.drag_start_x.map(|sx| {
            let tx = app_state.mouse_x;
            let (sx, tx) = (sx.min(tx), sx.max(tx));
            let duration = app_state.x_to_time(tx) - app_state.x_to_time(sx);
            (sx, tx, duration * ratio)
        });
        for (sx, tx, beat) in notes.merge(drag) {
            if tx < 0.0 || 2880.0 < sx {
                continue;
            }
            let x = (sx + tx) as i32 / 2;
            // let half_w = w as i32 / 2 + 5;
            let half_w = 60;
            while let Some(p) = heap.peek_mut() {
                if p.0 .0 <= x - half_w {
                    heap2.push(Reverse(PeekMut::pop(p).1));
                } else {
                    break;
                }
            }
            let slot = heap2.pop().map_or(heap.len(), |x| x.0);
            heap.push((Reverse(x + half_w), slot));
            let y = 450 + 108 * slot as i32;
            let rect = Rect::new(sx as i32, y - 5, (tx - sx) as u32, 10);
            canvas.set_draw_color(Color::GRAY);
            canvas.draw_rect(rect)?;

            draw_range_text(
                canvas,
                texture_creator,
                font,
                beat,
                app_state.duration_error_rate,
                (x, y),
                Color::YELLOW,
                3,
            )?;
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
        let messages = [
            format!("Jump to: {}", app_state.jump_combo),
            format!("note_hit_x = {:?}", app_state.note_hit_x),
            format!("speed_factor = {:?}", app_state.speed_factor),
            format!("speed_error_rate = {:?} %", app_state.speed_error_rate),
            format!(
                "duration_error_rate = {:?} %",
                app_state.duration_error_rate
            ),
        ];
        let mut y = 0;
        for message in messages {
            let text_surface = font
                .render(&message)
                .solid(Color::WHITE)
                .map_err(|e| e.to_string())?;
            let (w, h) = (text_surface.width(), text_surface.height());
            let text_texture = texture_creator
                .create_texture_from_surface(text_surface)
                .map_err(|e| e.to_string())?;
            let rect = Rect::new(0, y, w, h);
            canvas.copy(&text_texture, None, rect)?;
            y += h as i32;
        }
    }

    canvas.set_clip_rect(Rect::new(0, 0, 2880, 90));
    for ((sx, sy, res), (tx, ty, _)) in data
        .integrals
        .iter()
        .flat_map(|x| &x.results)
        .map(|(&pts, res)| {
            (
                app_state.to_x(linop_map(&data.pts_to_time, pts)) as i32,
                map_float(res.top_left as _, 900.0, 5000.0, 30.0, 90.0) as i32,
                res,
            )
        })
        .tuple_windows()
    {
        if tx < -100 || 3180 < sx {
            continue;
        }
        canvas.set_draw_color(Color::GREEN);
        canvas.draw_line((sx, sy), (tx, ty))?;
        if let Some(color) = match res.bottom {
            1 => Some(Color::RGB(255, 0, 0)),
            2 => Some(Color::RGB(255, 128, 0)),
            3 => Some(Color::RGB(0, 128, 0)),
            4 => Some(Color::RGB(128, 255, 128)),
            5 => Some(Color::RGB(0, 255, 0)),
            6 => Some(Color::RGB(0, 0, 128)),
            7 => Some(Color::RGB(128, 128, 255)),
            8 => Some(Color::RGB(0, 0, 255)),
            _ => None,
        } {
            let rect = Rect::new(sx, 10, (tx - sx) as _, 10);
            canvas.set_draw_color(color);
            canvas.fill_rect(rect)?;
            canvas.set_draw_color(Color::WHITE);
            canvas.draw_rect(rect)?;
        }
    }
    canvas.set_clip_rect(None);

    canvas.set_draw_color(Color::WHITE);
    canvas.draw_line(
        (app_state.mouse_x as i32, 0),
        (app_state.mouse_x as i32, 1620),
    )?;

    canvas.present();
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn draw_range_text(
    canvas: &mut WindowCanvas,
    texture_creator: &TextureCreator<WindowContext>,
    font: &Font,
    value: f64,
    error_rate: PreciseDecimal,
    (x, y): (i32, i32),
    color: Color,
    width: usize,
) -> Result<(), String> {
    let rate = error_rate.value() / 100.;
    let dark = Color::RGB(color.r / 2, color.g / 2, color.b / 2);
    for (value, y, color) in [
        (value * (1. - rate), y - 36, dark),
        (value, y, color),
        (value * (1. + rate), y + 36, dark),
    ] {
        let text_surface = font
            .render(&format!("{:.width$}", value, width = width))
            .solid(color)
            .map_err(|e| e.to_string())?;
        let (w, h) = (text_surface.width(), text_surface.height());
        let text_texture = texture_creator
            .create_texture_from_surface(text_surface)
            .map_err(|e| e.to_string())?;
        let rect = Rect::from_center((x, y), w, h);
        canvas.copy(&text_texture, None, rect)?;
    }
    Ok(())
}

fn linop_map(map: &BTreeMap<i64, f64>, pts: i64) -> f64 {
    assert!(map.len() >= 2);
    let mut nexts = map.range(pts..);
    let mut prevs = map.range(..pts).rev();
    let ((&sx, &sy), (&tx, &ty)) = match (prevs.next(), nexts.next()) {
        (Some(s), Some(t)) => (s, t),
        (None, Some(s)) => (s, nexts.next().unwrap()),
        (Some(t), None) => (prevs.next().unwrap(), t),
        (None, None) => unreachable!("map.len() >= 1"),
    };
    map_float(pts as _, sx as _, tx as _, sy as _, ty as _)
}
