use crate::audio::{AudioManager, SoundBuffer};
use crate::errors::{new_sdl_error, TaikoError, TaikoErrorCause};
use sdl2::image::LoadTexture;
use sdl2::render::{Texture, TextureCreator, TextureQuery};
use sdl2::video::WindowContext;
use std::path::Path;

pub struct Assets<'a> {
    pub textures: Textures<'a>,
    pub chunks: Chunks,
}

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
}

pub struct Chunks {
    pub sound_don: SoundBuffer,
    pub sound_ka: SoundBuffer,
}

impl<'a> Assets<'a> {
    pub fn new<'b>(
        texture_creator: &'a TextureCreator<WindowContext>,
        audio_manager: &'b AudioManager,
    ) -> Result<Assets<'a>, TaikoError> {
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
        };

        let snd_dir = assets_dir.join("snd");
        let load_sound = |filename| {
            SoundBuffer::load(
                snd_dir.join(filename),
                audio_manager.stream_config.channels,
                audio_manager.stream_config.sample_rate,
            )
        };
        let chunks = Chunks {
            sound_don: load_sound("dong.ogg")?,
            sound_ka: load_sound("ka.ogg")?,
        };

        Ok(Assets { textures, chunks })
    }
}

fn load_texture_and_check_size<P: AsRef<Path>>(
    texture_creator: &TextureCreator<WindowContext>,
    path: P,
    required_dimensions: (u32, u32),
) -> Result<Texture, TaikoError> {
    let texture = texture_creator
        .load_texture(path)
        .map_err(|s| new_sdl_error("Failed to load background texture", s))?;
    match texture.query() {
        TextureQuery { width, height, .. } if (width, height) == required_dimensions => {}
        _ => {
            return Err(TaikoError {
                message: "Texture size of the background is invalid".to_string(),
                cause: TaikoErrorCause::InvalidResourceError,
            });
        }
    }
    Ok(texture)
}
