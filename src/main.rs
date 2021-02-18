use taiko_untitled::assets::Assets;
use taiko_untitled::errors::{
    new_config_error, new_sdl_canvas_error, new_sdl_error, new_sdl_window_error, TaikoError,
    TaikoErrorCause,
};
use taiko_untitled::game::game;
use taiko_untitled::mode::GameMode;
use taiko_untitled::pause::pause;

fn main() -> Result<(), TaikoError> {
    let config = taiko_untitled::config::get_config()
        .map_err(|e| new_config_error("Failed to load configuration", e))?;

    let tja_file_name = std::env::args().nth(1).ok_or_else(|| TaikoError {
        message: "Input file is not specified".to_owned(),
        cause: TaikoErrorCause::None,
    })?;

    let sdl_context =
        sdl2::init().map_err(|s| new_sdl_error("Failed to initialize SDL context", s))?;
    let video_subsystem = sdl_context
        .video()
        .map_err(|s| new_sdl_error("Failed to initialize video subsystem of SDL", s))?;
    let window = video_subsystem
        .window("", config.window.width, config.window.height)
        .allow_highdpi()
        .build()
        .map_err(|x| new_sdl_window_error("Failed to create main window", x))?;

    let event_subsystem = sdl_context
        .event()
        .map_err(|s| new_sdl_error("Failed to initialize event subsystem of SDL", s))?;
    let mut event_pump = sdl_context
        .event_pump()
        .map_err(|s| new_sdl_error("Failed to initialize event pump for SDL", s))?;

    let mut canvas = window.into_canvas();
    if config.window.vsync {
        canvas = canvas.present_vsync();
    }
    let mut canvas = canvas
        .build()
        .map_err(|e| new_sdl_canvas_error("Failed to create SDL canvas", e))?;
    match canvas.output_size() {
        Ok((width, height)) => {
            let scale = f32::min(width as f32 / 1920.0, height as f32 / 1080.0);
            if let Err(s) = canvas.set_scale(scale, scale) {
                eprintln!("Failed to scale the dimensions.  The drawing scale may not be valid.");
                eprintln!("Caused by: {}", s);
            }
        }
        Err(s) => {
            eprintln!("Failed to get the canvas dimension.  The drawing scale may not be valid.");
            eprintln!("Caused by: {}", s);
        }
    }
    let texture_creator = canvas.texture_creator();

    let mut timer_subsystem = sdl_context
        .timer()
        .map_err(|s| new_sdl_error("Failed to initialize timer subsystem of SDL", s))?;

    let audio_manager = taiko_untitled::audio::AudioManager::new()?;

    let mut assets = Assets::new(&texture_creator, &audio_manager)?;
    {
        let volume = config.volume.se / 100.0;
        assets.chunks.sound_don.set_volume(volume);
        assets.chunks.sound_ka.set_volume(volume);
        let volume = config.volume.song / 100.0;
        audio_manager.set_music_volume(volume)?;
    }

    let mut mode = GameMode::Play;
    loop {
        mode = match mode {
            GameMode::Play => game(
                &config,
                &mut canvas,
                &event_subsystem,
                &mut event_pump,
                &mut timer_subsystem,
                &audio_manager,
                &mut assets,
                &tja_file_name,
            )?,
            GameMode::Pause { song, path } => pause(
                &config,
                &mut canvas,
                &event_subsystem,
                &mut event_pump,
                &mut timer_subsystem,
                &audio_manager,
                &mut assets,
                path,
                song,
            )?,
            GameMode::Exit => break,
        }
    }

    Ok(())
}
