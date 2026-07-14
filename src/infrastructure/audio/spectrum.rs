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

/// Compute a mapping from bands to fractional FFT bin positions using a logarithmic scale.
/// This prevents bands from having "0 count" or being empty.
fn compute_band_to_bin(sample_rate: u32) -> [f32; BANDS] {
    let bin_width = sample_rate as f32 / FFT_SIZE as f32;
    let nyquist = sample_rate as f32 / 2.0;

    // Start logarithmic mapping from bin 1 up to nyquist frequency.
    let log_min = bin_width.ln();
    let log_max = nyquist.ln();

    let mut band_to_bin = [0.0f32; BANDS];
    for (b, val) in band_to_bin.iter_mut().enumerate() {
        let t = b as f32 / (BANDS - 1).max(1) as f32;
        let freq = (log_min + (log_max - log_min) * t).exp();
        let bin_pos = freq / bin_width;
        // Keep it within valid bins [1.0, NUM_BINS]
        *val = bin_pos.clamp(1.0, NUM_BINS as f32);
    }
    band_to_bin
}

fn get_fft_plan() -> Arc<dyn Fft<f32>> {
    static FFT_PLAN: OnceLock<Arc<dyn Fft<f32>>> = OnceLock::new();
    FFT_PLAN
        .get_or_init(|| {
            let mut planner = FftPlanner::new();
            planner.plan_fft_forward(FFT_SIZE)
        })
        .clone()
}

pub struct SpectrumSource<S: Source<Item = f32>> {
    inner: S,
    frame: Arc<Mutex<SpectrumFrame>>,
    buffer: [f32; FFT_SIZE],
    pos: usize,
    fft: Arc<dyn Fft<f32>>,
    band_to_bin: [f32; BANDS],
    fft_buf: Vec<Complex<f32>>,
    norm_peak: [f32; BANDS],
    vis_peak: [f32; BANDS],
}

impl<S: Source<Item = f32>> SpectrumSource<S> {
    pub fn new(source: S) -> (Self, Arc<Mutex<SpectrumFrame>>) {
        let frame = Arc::new(Mutex::new(SpectrumFrame::default()));
        let sr = source.sample_rate().get();
        let fft = get_fft_plan();
        let band_to_bin = compute_band_to_bin(sr);
        let this = Self {
            inner: source,
            frame: frame.clone(),
            buffer: [0.0; FFT_SIZE],
            pos: 0,
            fft,
            band_to_bin,
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

        // Precompute magnitude of each bin
        let mut fft_mag = [0.0f32; NUM_BINS + 1];
        for (bin, val) in fft_mag.iter_mut().enumerate() {
            *val = self.fft_buf[bin].norm().sqrt();
        }

        if let Ok(mut frame) = self.frame.lock() {
            for i in 0..BANDS {
                // Linearly interpolate the magnitude from the surrounding FFT bins
                let bin_pos = self.band_to_bin[i];
                let idx = bin_pos.floor() as usize;
                let frac = bin_pos - idx as f32;
                let next = (idx + 1).min(NUM_BINS);
                let val = fft_mag[idx] * (1.0 - frac) + fft_mag[next] * frac;

                // Peak normalization/scaling (keeps values dynamic)
                self.norm_peak[i] *= 0.995;
                self.norm_peak[i] = self.norm_peak[i].max(val);
                let normalized = if self.norm_peak[i] > 0.0 {
                    val / self.norm_peak[i]
                } else {
                    0.0
                };

                // Apply smoothing
                frame.bands[i] = frame.bands[i] * 0.7 + normalized * 0.3;

                // Peak decay tracking
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
