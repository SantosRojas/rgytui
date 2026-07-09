#![allow(dead_code)]

mod app;
mod application;
mod domain;
mod infrastructure;
mod interface;
mod shared;

use anyhow::Context;
use tracing_subscriber::EnvFilter;

use crate::app::App;
use crate::application::playback::PlaybackUseCase;
use crate::application::playlist::PlaylistUseCase;
use crate::application::search::SearchUseCase;
use crate::infrastructure::audio::mpv_backend::MpvBackend;
use crate::infrastructure::audio::rodio_backend::RodioBackend;
use crate::infrastructure::config::store::ConfigStore;
use crate::infrastructure::ytdlp::client::YtDlpClient;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,symphonia=error")),
        )
        .init();

    let config = ConfigStore::new().context("Failed to load config")?;
    let ytdlp = YtDlpClient::new();
    let audio = RodioBackend::new().context("Failed to initialize audio output")?;
    let mpv = MpvBackend::new();

    let mut playlist = PlaylistUseCase::new();
    let saved = config.load_playlist();
    for song in saved.songs {
        playlist.add(song);
    }

    let playback = PlaybackUseCase::new(ytdlp, audio, mpv);
    let search = SearchUseCase::new(YtDlpClient::new());

    let mut app = App::new(playback, search, playlist, config);

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
