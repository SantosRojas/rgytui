pub mod mpv_backend;
pub mod rodio_backend;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AudioMode {
    Audio,
    Video,
}
