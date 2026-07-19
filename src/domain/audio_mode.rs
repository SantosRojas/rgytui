#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AudioMode {
    Audio,
    Video,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_mode_exists_as_enum() {
        let audio = AudioMode::Audio;
        let video = AudioMode::Video;
        assert_ne!(audio, video);
    }

    #[test]
    fn audio_mode_is_clone_copy_debug_partialeq() {
        let mode = AudioMode::Audio;
        let cloned = mode;
        assert_eq!(mode, cloned);
    }

    #[test]
    fn audio_mode_variants_display() {
        assert_eq!(format!("{:?}", AudioMode::Audio), "Audio");
        assert_eq!(format!("{:?}", AudioMode::Video), "Video");
    }
}
