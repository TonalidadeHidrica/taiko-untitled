use crate::errors::{TaikoError, TaikoErrorCause};
use sdl2::image::LoadTexture;
use sdl2::mixer::Chunk;
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
}

pub struct Chunks {
    pub sound_don: Chunk,
    pub sound_ka: Chunk,
}

impl<'a> Assets<'a> {
    pub fn new(
        texture_creator: &'a TextureCreator<WindowContext>,
    ) -> Result<Assets<'a>, TaikoError> {
        let tc = texture_creator;
        let assets_dir = Path::new("assets");
        let img_dir = assets_dir.join("img");
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
        };
        let snd_dir = assets_dir.join("snd");
        let ret = Assets {
            textures,
            chunks: Chunks {
                sound_don: Chunk::from_file(snd_dir.join("dong.ogg"))
                    .map_err(|s| TaikoError::new_sdl_error("Failed to load 'don' sound", s))?,
                sound_ka: Chunk::from_file(snd_dir.join("ka.ogg"))
                    .map_err(|s| TaikoError::new_sdl_error("Failed to load 'ka' sound", s))?,
            },
        };
        Ok(ret)
    }
}

fn load_texture_and_check_size<P: AsRef<Path>>(
    texture_creator: &TextureCreator<WindowContext>,
    path: P,
    required_dimensions: (u32, u32),
) -> Result<Texture, TaikoError> {
    let texture = texture_creator
        .load_texture(path)
        .map_err(|s| TaikoError::new_sdl_error("Failed to load background texture", s))?;
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
