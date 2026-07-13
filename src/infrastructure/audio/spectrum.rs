use std::num::NonZero;
use std::sync::{Arc, Mutex};
use std::sync::OnceLock;
use std::time::Duration;

use num_complex::Complex;
use rodio::Source;
use rustfft::{Fft, FftPlanner};

const FFT_SIZE: usize = 256;
pub const BANDS: usize = 32;
const NUM_BINS: usize = FFT_SIZE / 2;

#[derive(Clone, Copy, Debug, Default)]
pub struct SpectrumFrame {
    pub bands: [f32; BANDS],
    pub peaks: [f32; BANDS],
}

fn hann_window() -> &'static [f32; FFT_SIZE] {
    static WINDOW: OnceLock<[f32; FFT_SIZE]> = OnceLock::new();
    WINDOW.get_or_init(|| {
        let mut w = [0.0; FFT_SIZE];
        for (i, coeff) in w.iter_mut().enumerate() {
            *coeff = 0.5
                * (1.0
                    - (2.0 * std::f32::consts::PI * i as f32 / (FFT_SIZE - 1) as f32).cos());
        }
        w
    })
}

fn compute_bin_to_band(sample_rate: u32) -> [u8; NUM_BINS + 1] {
    let bin_width = sample_rate as f32 / FFT_SIZE as f32;
    let nyquist = sample_rate as f32 / 2.0;
    let log_min = bin_width.ln();
    let log_max = nyquist.ln();

    let mut boundaries = [0.0f32; BANDS + 1];
    for (b, bound) in boundaries.iter_mut().enumerate() {
        let t = b as f32 / BANDS as f32;
        *bound = (log_min + (log_max - log_min) * t).exp();
    }

    let mut map = [0u8; NUM_BINS + 1];
    for (bin, entry) in map.iter_mut().enumerate().skip(1) {
        let freq = bin as f32 * bin_width;
        for b in 0..BANDS {
            if freq >= boundaries[b] && freq < boundaries[b + 1] {
                *entry = b as u8;
                break;
            }
        }
    }
    map
}

pub struct SpectrumSource<S: Source<Item = f32>> {
    inner: S,
    frame: Arc<Mutex<SpectrumFrame>>,
    buffer: [f32; FFT_SIZE],
    pos: usize,
    fft: Arc<dyn Fft<f32>>,
    bin_to_band: [u8; NUM_BINS + 1],
    fft_buf: Vec<Complex<f32>>,
    norm_peak: [f32; BANDS],
    vis_peak: [f32; BANDS],
}

impl<S: Source<Item = f32>> SpectrumSource<S> {
    pub fn new(source: S) -> (Self, Arc<Mutex<SpectrumFrame>>) {
        let frame = Arc::new(Mutex::new(SpectrumFrame::default()));
        let sr = source.sample_rate().get();
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(FFT_SIZE);
        let bin_to_band = compute_bin_to_band(sr);
        let this = Self {
            inner: source,
            frame: frame.clone(),
            buffer: [0.0; FFT_SIZE],
            pos: 0,
            fft,
            bin_to_band,
            fft_buf: vec![Complex::new(0.0, 0.0); FFT_SIZE],
            norm_peak: [0.0; BANDS],
            vis_peak: [0.0; BANDS],
        };
        (this, frame)
    }

    fn flush_buffer(&mut self) {
        let window = hann_window();
        for (i, &s) in self.buffer.iter().enumerate() {
            self.fft_buf[i] = Complex::new(s * window[i], 0.0);
        }
        self.fft.process(&mut self.fft_buf);

        let mut band_sum = [0.0f32; BANDS];
        let mut band_count = [0usize; BANDS];
        for bin in 1..=NUM_BINS {
            let mag = self.fft_buf[bin].norm().sqrt();
            let b = self.bin_to_band[bin] as usize;
            band_sum[b] += mag;
            band_count[b] += 1;
        }

        if let Ok(mut frame) = self.frame.lock() {
            for i in 0..BANDS {
                let avg = if band_count[i] > 0 {
                    band_sum[i] / band_count[i] as f32
                } else {
                    0.0
                };

                self.norm_peak[i] *= 0.995;
                self.norm_peak[i] = self.norm_peak[i].max(avg);
                let normalized = if self.norm_peak[i] > 0.0 {
                    avg / self.norm_peak[i]
                } else {
                    0.0
                };

                frame.bands[i] = frame.bands[i] * 0.7 + normalized * 0.3;

                self.vis_peak[i] *= 0.98;
                self.vis_peak[i] = self.vis_peak[i].max(frame.bands[i]);
                frame.peaks[i] = self.vis_peak[i];
            }
        }
    }
}

impl<S: Source<Item = f32>> Iterator for SpectrumSource<S> {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        let sample = self.inner.next()?;
        self.buffer[self.pos] = sample;
        self.pos += 1;
        if self.pos >= FFT_SIZE {
            self.flush_buffer();
            self.pos = 0;
        }
        Some(sample)
    }
}

impl<S: Source<Item = f32>> Source for SpectrumSource<S> {
    fn current_span_len(&self) -> Option<usize> {
        self.inner.current_span_len()
    }

    fn channels(&self) -> NonZero<u16> {
        self.inner.channels()
    }

    fn sample_rate(&self) -> NonZero<u32> {
        self.inner.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }
}
