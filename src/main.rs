mod app;
mod application;
mod domain;
mod infrastructure;
mod interface;
mod shared;

use std::sync::Arc;

use anyhow::Context;
use tracing_subscriber::EnvFilter;

use crate::app::App;
use crate::application::playback::PlaybackUseCase;
use crate::application::playlist::PlaylistUseCase;
use crate::application::ports::{AudioPlaybackPort, ConfigPort, DownloaderPort, I18nPort, MediaSearchPort};
use crate::application::search::SearchUseCase;
use crate::infrastructure::audio::mpv_backend::MpvAdapter;
use crate::infrastructure::audio::rodio_backend::RodioAdapter;
use crate::infrastructure::config::store::ConfigAdapter;
use crate::infrastructure::ytdlp::client::YtDlpAdapter;
use crate::interface::i18n::Translations;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,symphonia=error")),
        )
        .init();

    let config = ConfigAdapter::new().await.context("Failed to load config")?;
    let settings = config.settings().clone();
    let ytdlp = YtDlpAdapter::new();
    let audio: Box<dyn AudioPlaybackPort> = Box::new(RodioAdapter::new().context("Failed to initialize audio output")?);
    let mpv = MpvAdapter::new();

    let mut playlist = PlaylistUseCase::new();
    let saved = config.load_playlist().await;
    for song in saved.songs {
        playlist.add(song);
    }

    let downloader_port: Arc<dyn DownloaderPort> = Arc::new(ytdlp.clone());
    let search_port: Arc<dyn MediaSearchPort> = Arc::new(ytdlp);

    let playback = PlaybackUseCase::new(downloader_port, audio, mpv);
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
