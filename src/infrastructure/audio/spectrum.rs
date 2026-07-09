use std::num::NonZero;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rodio::Source;

const BANDS: usize = 16;
const SAMPLES_PER_BAND: usize = 16;
const BUFFER_SIZE: usize = BANDS * SAMPLES_PER_BAND;

pub struct SpectrumSource<S: Source<Item = f32>> {
    inner: S,
    bands: Arc<Mutex<[f32; BANDS]>>,
    sample_buf: [f32; BUFFER_SIZE],
    pos: usize,
}

impl<S: Source<Item = f32>> SpectrumSource<S> {
    pub fn new(source: S) -> (Self, Arc<Mutex<[f32; BANDS]>>) {
        let bands = Arc::new(Mutex::new([0.0f32; BANDS]));
        let this = Self {
            inner: source,
            bands: bands.clone(),
            sample_buf: [0.0; BUFFER_SIZE],
            pos: 0,
        };
        (this, bands)
    }

    fn flush_buffer(&mut self) {
        let mut sums = [0.0f32; BANDS];
        for (i, &sample) in self.sample_buf.iter().enumerate() {
            sums[i / SAMPLES_PER_BAND] += sample.abs();
        }
        if let Ok(mut bands) = self.bands.lock() {
            for (i, sum) in sums.iter().enumerate() {
                let avg = sum / BUFFER_SIZE as f32;
                bands[i] = (bands[i] * 0.7 + avg * 0.3).min(1.0);
            }
        }
    }
}

impl<S: Source<Item = f32>> Iterator for SpectrumSource<S> {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        let sample = self.inner.next()?;
        self.sample_buf[self.pos] = sample;
        self.pos += 1;
        if self.pos >= BUFFER_SIZE {
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
