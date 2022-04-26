use std::path::PathBuf;

use anyhow::anyhow;
use clap::Parser;
use config::Config;
use fs_err::File;
use itertools::{chain, Itertools};
use ordered_float::OrderedFloat;
use sdl2::{
    event::Event,
    keyboard::{Keycode, Scancode},
    mouse::MouseWheelDirection,
    pixels::Color,
    rect::{Point, Rect},
    render::{TextureCreator, WindowCanvas},
    ttf::Font,
    video::WindowContext,
};
use taiko_untitled::{analyze::NotePositionsResult, video_analyzer_assets::get_single_note_color};

#[derive(Parser)]
struct Opts {
    json_path: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    let mut config = Config::default();
    let config = config.merge(config::File::with_name("config.toml"))?;

    let data: NotePositionsResult = serde_json::from_reader(File::open(&opts.json_path)?)?;

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

    let y_factor = canvas.window().drawable_size().1 as f64 / 1080.0;
    let mut app_state = AppState {
        origin_x: 0.0,
        scale_x: canvas.window().drawable_size().0 as f64 / 1920.0,
        origin_y: -2680.0 * y_factor,
        scale_y: y_factor / 64.0,

        selected_points: vec![],
        mouse_over_point: None,

        show_grid: false,
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
        update_mouse_over(&data, (mouse_x, mouse_y), &mut app_state);
        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. } => break 'main,
                Event::MouseWheel {
                    x, y, direction, ..
                } => {
                    let x = x as f64;
                    let y = y as f64;

                    for (origin, scale, mouse) in chain!(
                        shift.then(|| (&mut app_state.origin_y, &mut app_state.scale_y, mouse_y)),
                        alt.then(|| (&mut app_state.origin_x, &mut app_state.scale_x, mouse_x)),
                    ) {
                        let scale_factor = 1.05f64.powf(-y);
                        *origin = mouse + (*origin - mouse) * scale_factor;
                        *scale *= scale_factor;
                    }
                    if !shift && !alt {
                        let sign = match direction {
                            MouseWheelDirection::Flipped => -1.0,
                            _ => 1.0,
                        };
                        app_state.origin_x -= x * 10.0;
                        app_state.origin_y -= y * 10.0 * sign;
                    }
                }
                Event::MouseButtonDown { .. } => {
                    if let Some(mouse_over_point) = app_state.mouse_over_point {
                        app_state.selected_points.push(mouse_over_point);
                    }
                }
                Event::KeyDown {
                    keycode: Some(keycode),
                    ..
                } => match keycode {
                    Keycode::Escape => app_state.selected_points.clear(),
                    Keycode::G => app_state.show_grid = !app_state.show_grid,
                    _ => (),
                },
                _ => {}
            }
        }

        draw(&mut canvas, &texture_creator, &font, &data, &app_state)
            .map_err(|e| anyhow!("{}", e))?;
    }

    Ok(())
}

struct AppState {
    origin_x: f64,
    scale_x: f64,
    origin_y: f64,
    scale_y: f64,

    selected_points: Vec<(i64, f64)>,
    mouse_over_point: Option<(i64, f64)>,

    show_grid: bool,
}
impl AppState {
    fn to_x(&self, note_x: f64) -> f64 {
        self.origin_x + note_x * self.scale_x
    }
    fn to_y(&self, pts: i64) -> f64 {
        self.origin_y + pts as f64 * self.scale_y
    }
    #[allow(unused)]
    fn x_to_note_x(&self, x: f64) -> f64 {
        (x - self.origin_x) / self.scale_x
    }
    fn y_to_pts(&self, y: f64) -> i64 {
        ((y - self.origin_y) / self.scale_y) as _
    }
}

fn update_mouse_over(data: &NotePositionsResult, mouse: (f64, f64), app_state: &mut AppState) {
    let pts = app_state.y_to_pts(mouse.1);
    app_state.mouse_over_point = data
        .results
        .range(pts - 16384..=pts + 16384)
        .flat_map(|(&pts, v)| v.notes.iter().map(move |n| (pts, n.note_x())))
        .filter_map(|(pts, note_x)| {
            let d = (app_state.to_x(note_x) - mouse.0).powi(2)
                + (app_state.to_y(pts) - mouse.1).powi(2);
            (d <= 256.0).then(|| (pts, note_x, OrderedFloat::from(d)))
        })
        .min_by_key(|x| x.2)
        .map(|x| (x.0, x.1));
}

fn draw(
    canvas: &mut WindowCanvas,
    texture_creator: &TextureCreator<WindowContext>,
    font: &Font,
    data: &NotePositionsResult,
    app_state: &AppState,
) -> Result<(), String> {
    canvas.set_draw_color(Color::BLACK);
    canvas.clear();

    for (&pts, frame) in &data.results {
        let y = app_state.to_y(pts);
        if app_state.show_grid {
            canvas.set_draw_color(Color::GRAY);
            canvas.draw_line(
                (0, y as i32),
                (canvas.window().drawable_size().0 as i32, y as i32),
            )?;
        }
        for note in &frame.notes {
            let x = app_state.to_x(note.note_x());
            let rect = Rect::from_center((x as i32, y as i32), 9, 9);
            canvas.set_draw_color(get_single_note_color(note.kind));
            canvas.fill_rect(rect)?;
        }
    }

    if let Some((pts, note_x)) = app_state.mouse_over_point {
        let x = app_state.to_x(note_x);
        let y = app_state.to_y(pts);
        let rect = Rect::from_center((x as i32, y as i32), 20, 20);
        canvas.set_draw_color(Color::YELLOW);
        canvas.draw_rect(rect)?;
    }

    canvas.set_draw_color(Color::GREEN);
    let points = app_state
        .selected_points
        .iter()
        .map(|&(pts, note_x)| Point::new(app_state.to_x(note_x) as i32, app_state.to_y(pts) as i32))
        .collect_vec();
    canvas.draw_lines(&points[..])?;

    for (((_, note_x_s), &s), ((_, note_x_t), &t)) in app_state
        .selected_points
        .iter()
        .zip(&points)
        .tuple_windows()
    {
        let x = (s.x + t.x) / 2;
        let y = (s.y + t.y) / 2;
        let text_surface = font
            .render(&format!("{:.1}", note_x_s - note_x_t))
            .solid(Color::WHITE)
            .map_err(|e| e.to_string())?;
        let (w, h) = (text_surface.width(), text_surface.height());
        let text_texture = texture_creator
            .create_texture_from_surface(text_surface)
            .map_err(|e| e.to_string())?;
        let rect = Rect::from_center((x, y), w, h);
        canvas.copy(&text_texture, None, rect)?;
    }

    canvas.present();
    Ok(())
}
