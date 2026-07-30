mod app;
mod application;
mod domain;
mod infrastructure;
mod interface;
mod shared;
mod uninstall;
mod update;

use std::sync::Arc;

use anyhow::Context;
use tracing_subscriber::EnvFilter;

use crate::app::App;
use crate::application::playback::PlaybackUseCase;
use crate::application::playlist::PlaylistUseCase;
use crate::application::ports::{AudioPlaybackPort, ConfigPort, DownloaderPort, I18nPort, MediaSearchPort};
use crate::application::search::SearchUseCase;
use crate::domain::audio_mode::AudioMode;
use crate::infrastructure::audio::mpv_backend::MpvAdapter;
use crate::infrastructure::audio::rodio_backend::{NoopAudioAdapter, RodioAdapter};
use crate::infrastructure::config::store::ConfigAdapter;
use crate::infrastructure::ytdlp::client::YtDlpAdapter;
use crate::interface::i18n::Translations;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // Handle subcommands before starting the TUI
    let args: Vec<String> = std::env::args().collect();
    if let Some("uninstall") = args.get(1).map(|s| s.as_str()) {
        return crate::uninstall::run_uninstall();
    }
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,symphonia=error")),
        )
        .init();

    let mut config = ConfigAdapter::new().await.context("Failed to load config")?;
    let settings = config.settings().clone();
    let ytdlp = YtDlpAdapter::new();
    let audio: Box<dyn AudioPlaybackPort> = match RodioAdapter::new() {
        Ok(a) => Box::new(a),
        Err(e) => {
            tracing::warn!("Audio output unavailable ({}), running without sound. Use audio mode  \
                           to play audio once a device is available.", e);
            Box::new(NoopAudioAdapter)
        }
    };
    let mpv = MpvAdapter::new();

    let playlist = PlaylistUseCase::new();

    let downloader_port: Arc<dyn DownloaderPort> = Arc::new(ytdlp.clone());
    let search_port: Arc<dyn MediaSearchPort> = Arc::new(ytdlp);

    // Fall back to Audio if mpv is not installed (e.g. user had legacy config with audio_mode: true)
    // Also persist the corrected audio_mode to config so the warning goes away permanently.
    let initial_mode = if settings.audio_mode && !MpvAdapter::is_mpv_installed().await {
        tracing::warn!("Video mode configured but mpv is not installed. Falling back to Audio.");
        config.settings_mut().audio_mode = false;
        if let Err(e) = config.save_settings().await {
            tracing::warn!("Failed to persist corrected audio_mode: {}", e);
        }
        AudioMode::Audio
    } else {
        AudioMode::from_bool(settings.audio_mode)
    };
    let playback = PlaybackUseCase::new(downloader_port, audio, mpv, initial_mode);
    let search = SearchUseCase::new(search_port);
    let config_port: Box<dyn ConfigPort> = Box::new(config);

    // Determine language from settings, with system locale detection as default
    let language = if settings.language == "en" {
        Translations::detect_locale()
    } else {
        settings.language.clone()
    };
    let i18n: Arc<dyn I18nPort> = Arc::new(Translations::load(&language));

    let mut app = App::new(playback, search, playlist, config_port, i18n).await;

    match app.run().await {
        Ok(()) => {
            tracing::info!("Application exited cleanly");
            Ok(())
        }
        Err(e) => {
            tracing::error!("Application error: {}", e);
            eprintln!("Error: {}", e);
            Err(e)
        }
    }
}
