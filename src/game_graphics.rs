use crate::assets::Assets;
use crate::errors::{new_sdl_error, SdlError, TaikoError};
use crate::game_manager::{FlyingNote, Judge, JudgeStr};
use crate::structs::{
    just::{Note, NoteContent, RendaContent, RendaKind},
    BarLine, BarLineKind, Bpm, BranchType, NoteColor, NoteSize, SingleNoteKind,
};
use enum_map::EnumMap;
use num::clamp;
use sdl2::rect::Rect;
use sdl2::render::WindowCanvas;
use sdl2::{pixels::Color, render::Texture};
use std::borrow::Borrow;

pub fn game_rect() -> Rect {
    Rect::new(498, 288, 1422, 195)
}

pub fn draw_background(canvas: &mut WindowCanvas, assets: &Assets) -> Result<(), SdlError> {
    canvas.set_draw_color(Color::RGBA(20, 20, 20, 0));
    canvas.clear();
    canvas.copy(
        &assets.textures.background,
        None,
        Some(Rect::new(0, 0, 1920, 1080)),
    )?;
    Ok(())
}

#[derive(Clone, Copy, Default)]
pub struct BranchAnimationState {
    switch_time: f64,
    branch_before: BranchType,
    branch_after: BranchType,
}

impl BranchAnimationState {
    pub fn new(branch: BranchType) -> Self {
        Self {
            switch_time: 0.0,
            branch_before: branch,
            branch_after: branch,
        }
    }

    pub fn set(&mut self, branch: BranchType, time: f64) {
        self.branch_before = self.branch_after;
        self.branch_after = branch;
        self.switch_time = time;
    }

    pub fn get(&self) -> BranchType {
        self.branch_after
    }
}

/// Branch overleay effect
pub fn draw_branch_overlay(
    canvas: &mut WindowCanvas,
    music_position: f64,
    score_rect: Rect,
    bs: &BranchAnimationState,
) -> Result<(), TaikoError> {
    // TODO color for master course is wrong
    canvas.set_blend_mode(sdl2::render::BlendMode::Add);
    canvas.set_draw_color(interpolate_color(
        branch_overlay_color(bs.branch_before),
        branch_overlay_color(bs.branch_after),
        clamp((music_position - bs.switch_time) * 60.0 / 20.0, 0.0, 1.0),
    ));
    canvas
        .fill_rect(score_rect)
        .map_err(|e| new_sdl_error("Failed to draw branch overlay", e))?;
    canvas.set_blend_mode(sdl2::render::BlendMode::None);
    Ok(())
}

fn branch_overlay_color(branch_type: BranchType) -> Color {
    match branch_type {
        BranchType::Normal => Color::RGB(0, 0, 0),
        BranchType::Expert => Color::RGB(8, 38, 55),
        BranchType::Master => Color::RGB(58, 0, 53),
    }
}

pub fn draw_bar_lines<'a, I>(
    canvas: &mut WindowCanvas,
    music_position: f64,
    bar_lines: I,
) -> Result<(), TaikoError>
where
    I: Iterator<Item = &'a BarLine>,
{
    let mut sorted_bar_lines = EnumMap::<_, Vec<_>>::default();
    for bar_line in bar_lines {
        let x = get_x(music_position, bar_line.time, bar_line.scroll_speed) as i32;
        if (0..=2000).contains(&x) {
            sorted_bar_lines[bar_line.kind].push(Rect::new(x + 96, 288, 3, 195));
        }
    }
    for (kind, rects) in sorted_bar_lines {
        match kind {
            BarLineKind::Normal => canvas.set_draw_color(Color::RGB(200, 200, 200)),
            BarLineKind::Branch => canvas.set_draw_color(Color::RGB(0xf3, 0xff, 0x55)),
        };
        canvas
            .fill_rects(&rects[..])
            .map_err(|e| new_sdl_error("Failed to draw bar lines", e))?;
    }
    Ok(())
}

pub fn draw_notes<I, N>(
    canvas: &mut WindowCanvas,
    assets: &Assets,
    music_position: f64,
    notes: I,
) -> Result<(), TaikoError>
where
    I: Iterator<Item = N>,
    N: Borrow<Note>,
{
    for note in notes {
        let note = note.borrow();
        match note.content {
            NoteContent::Single(single_note) => {
                let x = get_x(music_position, note.time, note.scroll_speed);
                draw_note(canvas, assets, &single_note.kind, x as i32, 288)?;
            }
            NoteContent::Renda(RendaContent {
                end_time,
                kind: RendaKind::Unlimited(renda),
                ..
            }) => {
                let (texture_left, texture_right) = match renda.size {
                    NoteSize::Small => (&assets.textures.renda_left, &assets.textures.renda_right),
                    NoteSize::Large => (
                        &assets.textures.renda_large_left,
                        &assets.textures.renda_large_right,
                    ),
                };
                // TODO coordinates calculations may lead to overflows
                let xs = get_x(music_position, note.time, note.scroll_speed) as i32;
                let xt = get_x(music_position, end_time, note.scroll_speed) as i32;
                canvas
                    .copy(
                        texture_right,
                        Rect::new(97, 0, 195 - 97, 195),
                        Rect::new(xt + 97, 288, 195 - 97, 195),
                    )
                    .map_err(|e| new_sdl_error("Failed to draw renda right", e))?;
                canvas
                    .copy(
                        texture_right,
                        Rect::new(0, 0, 97, 195),
                        Rect::new(xs + 97, 288, (xt - xs) as u32, 195),
                    )
                    .map_err(|e| new_sdl_error("Failed to draw renda center", e))?;
                canvas
                    .copy(texture_left, None, Rect::new(xs, 288, 195, 195))
                    .map_err(|e| new_sdl_error("Failed to draw renda left", e))?;
            }
            NoteContent::Renda(RendaContent {
                end_time,
                kind: RendaKind::Quota(..),
                ..
            }) => {
                let display_time = num::clamp(music_position, note.time, end_time);
                let x = get_x(music_position, display_time, note.scroll_speed) as i32;
                canvas
                    .copy(
                        &assets.textures.renda_left,
                        None,
                        Rect::new(x, 288, 195, 195),
                    )
                    .map_err(|e| new_sdl_error("Failed to draw renda left", e))?;
            }
        }
    }
    Ok(())
}

pub fn draw_note(
    canvas: &mut WindowCanvas,
    assets: &Assets,
    kind: &SingleNoteKind,
    x: i32,
    y: i32,
) -> Result<(), TaikoError> {
    let texture = match kind.color {
        NoteColor::Don => match kind.size {
            NoteSize::Small => &assets.textures.note_don,
            NoteSize::Large => &assets.textures.note_don_large,
        },
        NoteColor::Ka => match kind.size {
            NoteSize::Small => &assets.textures.note_ka,
            NoteSize::Large => &assets.textures.note_ka_large,
        },
    };
    canvas
        .copy(texture, None, Rect::new(x, y, 195, 195))
        .map_err(|e| new_sdl_error("Failed to draw a note", e))
}

pub fn draw_flying_notes<'a, I>(
    canvas: &mut WindowCanvas,
    assets: &Assets,
    music_position: f64,
    notes: I,
) -> Result<(), TaikoError>
where
    I: Iterator<Item = &'a FlyingNote>,
{
    for note in notes {
        // ends in 0.5 seconds
        let t = (music_position - note.time) * 60.0;
        if t >= 0.5 {
            // after 0.5 frames
            let x = 521.428 + 19.4211 * t + 1.75748 * t * t - 0.035165 * t * t * t;
            let y = 288.4 - 44.303 * t + 0.703272 * t * t + 0.0368848 * t * t * t
                - 0.000542067 * t * t * t * t;
            draw_note(canvas, assets, &note.kind, x as i32, y as i32)?;
        }
    }
    Ok(())
}

pub fn draw_judge_strs<'a, I>(
    canvas: &mut WindowCanvas,
    assets: &mut Assets,
    music_position: f64,
    judge_strs: I,
) -> Result<(), TaikoError>
where
    I: Iterator<Item = &'a JudgeStr>,
{
    for judge in judge_strs {
        // (552, 226)
        let (y, a) = match (music_position - judge.time) * 60.0 {
            t if t < 1.0 => (226.0 - 20.0 * t, t),
            t if t < 6.0 => (206.0 + 20.0 * (t - 1.0) / 5.0, 1.0),
            t if t < 14.0 => (226.0, 1.0),
            t => (226.0, (18.0 - t) / 4.0),
        };
        let texture = match judge.judge {
            Judge::Good => &mut assets.textures.judge_text_good,
            Judge::Ok => &mut assets.textures.judge_text_ok,
            Judge::Bad => &mut assets.textures.judge_text_bad,
        };
        texture.set_alpha_mod((a * 255.0) as u8);
        canvas
            .copy(texture, None, Some(Rect::new(552, y as i32, 135, 90)))
            .map_err(|e| new_sdl_error("Failed to draw judge str", e))?;
    }
    Ok(())
}

pub fn draw_combo(
    canvas: &mut WindowCanvas,
    textures: &[Texture],
    seconds_after_update: f64,
    digits: Vec<u32>,
) -> Result<(), TaikoError> {
    let w = (52.0 * digits.len() as f64).min(44.0 * 4.0);
    let x = 399.0 - w / 2.0;
    let w = w / digits.len() as f64;
    let yd = match seconds_after_update * 60.0 {
        t if t < 2.0 => t * 7.5,
        t if t < 9.0 => (9.0 - t) * 15.0 / 7.0,
        _ => 0.0,
    };
    for (i, t) in digits.iter().map(|&i| &textures[i as usize]).enumerate() {
        let x = x + w * i as f64 - w * 3.0 / 44.0;
        let rect = Rect::new(
            x as i32,
            (334.0 - yd) as i32,
            (w * 55.0 / 44.0) as u32,
            (77.0 + yd) as u32,
        );
        canvas
            .copy(t, None, rect)
            .map_err(|e| new_sdl_error("Failed to draw combo number", e))?;
    }
    Ok(())
}

pub fn draw_gauge(
    canvas: &mut WindowCanvas,
    assets: &Assets,
    gauge: u32,
    clear_count: u32,
    all_count: u32,
) -> Result<(), String> {
    canvas.copy(
        &assets.textures.gauge_left_base,
        None,
        Rect::new(726, 204, 1920, 78),
    )?;
    canvas.copy(
        &assets.textures.gauge_right_base,
        None,
        Rect::new(726 + clear_count as i32 * 21, 204, 1920, 78),
    )?;

    let gauge_count = clamp(gauge, 0, clear_count);
    let src = Rect::new(0, 0, 21 * gauge_count, 78);
    canvas.copy(
        &assets.textures.gauge_left_red,
        src,
        Rect::new(738, 204, src.width(), src.height()),
    )?;

    let src = Rect::new(
        21 * gauge_count as i32,
        0,
        21 * (clear_count - gauge_count),
        78,
    );
    canvas.copy(
        &assets.textures.gauge_left_dark,
        src,
        Rect::new(738 + src.x(), 204, src.width(), src.height()),
    )?;

    let max_width = 21 * (all_count - clear_count) - 6;
    let gauge_count = clamp(gauge, clear_count, all_count);
    let src = Rect::new(0, 0, max_width.min(21 * (gauge_count - clear_count)), 78);
    canvas.copy(
        &assets.textures.gauge_right_yellow,
        src,
        Rect::new(
            738 + clear_count as i32 * 21,
            204,
            src.width(),
            src.height(),
        ),
    )?;

    let src = Rect::new(
        max_width.min(21 * (gauge_count - clear_count)) as i32,
        0,
        max_width.min(21 * (all_count - gauge_count)),
        78,
    );
    canvas.copy(
        &assets.textures.gauge_right_dark,
        src,
        Rect::new(
            738 + clear_count as i32 * 21 + src.x(),
            204,
            src.width(),
            src.height(),
        ),
    )?;

    canvas.copy(
        &assets.textures.gauge_soul,
        None,
        Rect::new(1799, 215, 71, 63),
    )?;
    Ok(())
}

fn interpolate_color(color_zero: Color, color_one: Color, t: f64) -> Color {
    let Color {
        r: r0,
        g: g0,
        b: b0,
        a: a0,
    } = color_zero;
    let Color {
        r: r1,
        g: g1,
        b: b1,
        a: a1,
    } = color_one;
    let f = |x, y| clamp(x as f64 * (1.0 - t) + y as f64 * t, 0.0, 255.0) as u8;
    Color::RGBA(f(r0, r1), f(g0, g1), f(b0, b1), f(a0, a1))
}

fn get_x(music_position: f64, time: f64, scroll_speed: Bpm) -> f64 {
    let diff = time - music_position;
    520.0 + 1422.0 / 4.0 * diff / scroll_speed.beat_duration()
}
