use sdl2::image::LoadTexture;
use sdl2::pixels::Color;
use sdl2::render::{Texture, TextureCreator, TextureQuery};
use sdl2::video::WindowContext;
use std::fmt::Debug;
use std::path::Path;

use crate::structs::{NoteColor, NoteSize, SingleNoteKind};

pub struct Textures<'a> {
    pub background: Texture<'a>,
    pub note_don: Texture<'a>,
    pub note_ka: Texture<'a>,
    pub note_don_large: Texture<'a>,
    pub note_ka_large: Texture<'a>,
    pub renda_left: Texture<'a>,
    pub renda_right: Texture<'a>,
    pub renda_large_left: Texture<'a>,
    pub renda_large_right: Texture<'a>,
    pub judge_text_good: Texture<'a>,
    pub judge_text_ok: Texture<'a>,
    pub judge_text_bad: Texture<'a>,
    pub combo_nummber_white: Vec<Texture<'a>>,
    pub combo_nummber_silver: Vec<Texture<'a>>,
    pub combo_nummber_gold: Vec<Texture<'a>>,

    pub gauge_left_base: Texture<'a>,
    pub gauge_left_dark: Texture<'a>,
    pub gauge_left_red: Texture<'a>,
    pub gauge_right_base: Texture<'a>,
    pub gauge_right_dark: Texture<'a>,
    pub gauge_right_yellow: Texture<'a>,
}

impl<'a> Textures<'a> {
    pub fn new(texture_creator: &'a TextureCreator<WindowContext>) -> Result<Textures<'a>, String> {
        let assets_dir = Path::new("assets");

        let img_dir = assets_dir.join("img");
        let tc = texture_creator;
        let textures = Textures {
            background: load_texture_and_check_size(tc, img_dir.join("game_bg.png"), (1920, 1080))?,
            note_don: load_texture_and_check_size(tc, img_dir.join("note_don.png"), (195, 195))?,
            note_ka: load_texture_and_check_size(tc, img_dir.join("note_ka.png"), (195, 195))?,
            note_don_large: load_texture_and_check_size(
                tc,
                img_dir.join("note_don_large.png"),
                (195, 195),
            )?,
            note_ka_large: load_texture_and_check_size(
                tc,
                img_dir.join("note_ka_large.png"),
                (195, 195),
            )?,
            renda_left: load_texture_and_check_size(
                tc,
                img_dir.join("renda_left.png"),
                (195, 195),
            )?,
            renda_right: load_texture_and_check_size(
                tc,
                img_dir.join("renda_right.png"),
                (195, 195),
            )?,
            renda_large_left: load_texture_and_check_size(
                tc,
                img_dir.join("renda_large_left.png"),
                (195, 195),
            )?,
            renda_large_right: load_texture_and_check_size(
                tc,
                img_dir.join("renda_large_right.png"),
                (195, 195),
            )?,
            judge_text_good: load_texture_and_check_size(
                tc,
                img_dir.join("judge_text_good.png"),
                (135, 90),
            )?,
            judge_text_ok: load_texture_and_check_size(
                tc,
                img_dir.join("judge_text_ok.png"),
                (135, 90),
            )?,
            judge_text_bad: load_texture_and_check_size(
                tc,
                img_dir.join("judge_text_bad.png"),
                (135, 90),
            )?,
            combo_nummber_white: load_combo_textures(|i| {
                tc.load_texture(img_dir.join(format!("combo_number_white_{}.png", i)))
            })?,
            combo_nummber_silver: load_combo_textures(|i| {
                tc.load_texture(img_dir.join(format!("combo_number_silver_{}.png", i)))
            })?,
            combo_nummber_gold: load_combo_textures(|i| {
                tc.load_texture(img_dir.join(format!("combo_number_gold_{}.png", i)))
            })?,
            gauge_left_base: load_texture_and_check_size(
                tc,
                img_dir.join("gauge_left_base.png"),
                (1920, 78),
            )?,
            gauge_left_dark: load_texture_and_check_size(
                tc,
                img_dir.join("gauge_left_dark.png"),
                (1044, 78),
            )?,
            gauge_left_red: load_texture_and_check_size(
                tc,
                img_dir.join("gauge_left_red.png"),
                (1044, 78),
            )?,
            gauge_right_base: load_texture_and_check_size(
                tc,
                img_dir.join("gauge_right_base.png"),
                (1920, 78),
            )?,
            gauge_right_dark: load_texture_and_check_size(
                tc,
                img_dir.join("gauge_right_dark.png"),
                (1044, 78),
            )?,
            gauge_right_yellow: load_texture_and_check_size(
                tc,
                img_dir.join("gauge_right_yellow.png"),
                (1044, 78),
            )?,
        };

        Ok(textures)
    }
}

fn load_texture_and_check_size<P: AsRef<Path> + Debug + Clone>(
    texture_creator: &TextureCreator<WindowContext>,
    path: P,
    required_dimensions: (u32, u32),
) -> Result<Texture, String> {
    let texture = texture_creator.load_texture(path.clone())?;
    match texture.query() {
        TextureQuery { width, height, .. } if (width, height) == required_dimensions => {}
        _ => return Err(format!("Texture size of {:?} is invalid", path)),
    }
    Ok(texture)
}

fn load_combo_textures<'a, F>(to_texture: F) -> Result<Vec<Texture<'a>>, String>
where
    F: Fn(usize) -> Result<Texture<'a>, String>,
{
    (0..10)
        .map(to_texture)
        .collect::<Result<Vec<Texture<'a>>, String>>()
}

pub fn get_single_note_color(kind: SingleNoteKind) -> Color {
    match (kind.size, kind.color) {
        (NoteSize::Small, NoteColor::Don) => Color::RED,
        (NoteSize::Small, NoteColor::Ka) => Color::BLUE,
        (NoteSize::Large, NoteColor::Don) => Color::MAGENTA,
        (NoteSize::Large, NoteColor::Ka) => Color::CYAN,
    }
}
