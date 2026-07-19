pub const BANDS: usize = 32;

#[derive(Clone, Copy, Debug, Default)]
pub struct SpectrumFrame {
    pub bands: [f32; BANDS],
    pub peaks: [f32; BANDS],
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spectrum_frame_exists() {
        let frame = SpectrumFrame::default();
        assert_eq!(frame.bands.len(), BANDS);
        assert_eq!(frame.peaks.len(), BANDS);
    }

    #[test]
    fn spectrum_frame_is_clone_copy_debug() {
        let frame = SpectrumFrame::default();
        let cloned = frame;
        assert_eq!(frame.bands[0], cloned.bands[0]);
    }

    #[test]
    fn bands_is_correct_size() {
        assert_eq!(BANDS, 32);
    }

    #[test]
    fn spectrum_frame_default_values() {
        let frame = SpectrumFrame::default();
        for i in 0..BANDS {
            assert_eq!(frame.bands[i], 0.0, "band[{}] should be 0.0", i);
            assert_eq!(frame.peaks[i], 0.0, "peak[{}] should be 0.0", i);
        }
    }
}
