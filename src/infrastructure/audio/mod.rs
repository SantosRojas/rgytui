pub mod mpv_backend;
pub mod rodio_backend;
pub mod spectrum;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AudioMode {
    Audio,
    Video,
}
