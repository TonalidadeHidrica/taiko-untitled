use crate::errors::{TaikoError, TaikoErrorCause};
use sdl2::image::LoadTexture;
use sdl2::mixer::Chunk;
use sdl2::render::{Texture, TextureCreator, TextureQuery};
use sdl2::video::WindowContext;

pub struct Assets<'a> {
    pub textures: Textures<'a>,
    pub chunks: Chunks,
}

pub struct Textures<'a> {
    pub background: Texture<'a>,
}

pub struct Chunks {
    pub sound_don: Chunk,
    pub sound_ka: Chunk,
}

impl<'a> Assets<'a> {
    pub fn new(
        texture_creator: &'a TextureCreator<WindowContext>,
    ) -> Result<Assets<'a>, TaikoError> {
        let ret = Assets {
            textures: Textures {
                background: texture_creator
                    .load_texture("assets/img/game_bg.png")
                    .map_err(|s| {
                        TaikoError::new_sdl_error("Failed to load background texture", s)
                    })?,
            },
            chunks: Chunks {
                sound_don: Chunk::from_file("assets/snd/dong.ogg")
                    .map_err(|s| TaikoError::new_sdl_error("Failed to load 'don' sound", s))?,
                sound_ka: Chunk::from_file("assets/snd/ka.ogg")
                    .map_err(|s| TaikoError::new_sdl_error("Failed to load 'ka' sound", s))?,
            },
        };
        match ret.textures.background.query() {
            TextureQuery {
                width: 1920,
                height: 1080,
                ..
            } => {}
            _ => {
                return Err(TaikoError {
                    message: "Texture size of the background is invalid".to_string(),
                    cause: TaikoErrorCause::InvalidResourceError,
                });
            }
        }
        Ok(ret)
    }
}
