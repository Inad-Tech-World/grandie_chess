//! Recording the microphone natively (cpal: Core Audio on macOS, WASAPI on
//! Windows). The web view's own speech APIs are unreliable inside the app on
//! macOS, so voice input is captured here and sent for transcription as a
//! 16 kHz mono WAV.

use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample};

use crate::error::{Error, Result};

/// Recording stops by itself after this long.
pub const MAX_SECONDS: f32 = 30.0;
/// Shorter than this is treated as "nothing said".
pub const MIN_SECONDS: f32 = 0.35;
const TARGET_RATE: u32 = 16_000;

/// Mono samples (-1..1) at `sample_rate`.
#[derive(Debug, Clone, Default)]
pub struct Recording {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

impl Recording {
    pub fn seconds(&self) -> f32 {
        if self.sample_rate == 0 {
            0.0
        } else {
            self.samples.len() as f32 / self.sample_rate as f32
        }
    }

    /// Loudest short window, 0..1; near zero means silence.
    pub fn peak_level(&self) -> f32 {
        let window = (self.sample_rate / 20).max(1) as usize;
        self.samples.chunks(window).map(rms).fold(0.0, f32::max)
    }
}

fn rms(xs: &[f32]) -> f32 {
    if xs.is_empty() {
        return 0.0;
    }
    (xs.iter().map(|x| x * x).sum::<f32>() / xs.len() as f32).sqrt()
}

struct Active {
    stop: mpsc::Sender<()>,
    done: mpsc::Receiver<Result<Recording>>,
}

/// One recording at a time, on a thread that owns the audio stream (cpal
/// streams can't move between threads).
#[derive(Default)]
pub struct Recorder {
    active: Mutex<Option<Active>>,
}

impl Recorder {
    pub fn is_recording(&self) -> bool {
        self.active.lock().unwrap().is_some()
    }

    /// Start recording from the default microphone. `on_level` gets the
    /// input level (0..1) about ten times a second.
    pub fn start(&self, on_level: impl Fn(f32) + Send + Sync + 'static) -> Result<()> {
        let mut active = self.active.lock().unwrap();
        if active.is_some() {
            return Err(Error::other("already listening"));
        }
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let (done_tx, done_rx) = mpsc::channel::<Result<Recording>>();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<()>>();
        std::thread::Builder::new()
            .name("grandie-mic".into())
            .spawn(move || {
                let result = record_until_stopped(stop_rx, &ready_tx, on_level);
                if let Err(e) = &result {
                    let _ = ready_tx.send(Err(Error::other(e.to_string())));
                }
                let _ = done_tx.send(result);
            })
            .map_err(|e| Error::other(format!("could not start recording: {e}")))?;
        match ready_rx.recv_timeout(Duration::from_secs(10)) {
            Ok(Ok(())) => {
                *active = Some(Active { stop: stop_tx, done: done_rx });
                Ok(())
            }
            Ok(Err(e)) => Err(e),
            Err(_) => Err(Error::other("the microphone didn't start (is access allowed in your system's privacy settings?)")),
        }
    }

    /// Stop and return what was recorded. Blocks briefly.
    pub fn stop(&self) -> Result<Recording> {
        let active = self.active.lock().unwrap().take().ok_or_else(|| Error::other("not listening"))?;
        let _ = active.stop.send(());
        active.done.recv_timeout(Duration::from_secs(5)).map_err(|_| Error::other("the recording didn't finish"))?
    }

    /// Stop and throw the recording away.
    pub fn cancel(&self) {
        if let Some(active) = self.active.lock().unwrap().take() {
            let _ = active.stop.send(());
        }
    }
}

fn mic_error(e: impl std::fmt::Display) -> Error {
    Error::other(format!(
        "Couldn't use the microphone: {e}. On macOS, allow Grandie Chess under System Settings → Privacy & Security → Microphone; \
         on Windows, under Settings → Privacy & security → Microphone."
    ))
}

fn record_until_stopped(
    stop: mpsc::Receiver<()>,
    ready: &mpsc::Sender<Result<()>>,
    on_level: impl Fn(f32) + Send + Sync + 'static,
) -> Result<Recording> {
    let host = cpal::default_host();
    let device = host.default_input_device().ok_or_else(|| mic_error("no microphone found"))?;
    let supported = device.default_input_config().map_err(mic_error)?;
    let channels = supported.channels().max(1) as usize;
    let sample_rate = supported.sample_rate();
    let format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    let buffer: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::with_capacity(sample_rate as usize * 10)));
    let max_samples = (MAX_SECONDS * sample_rate as f32) as usize;
    let on_level = Arc::new(on_level);
    let stream = match format {
        SampleFormat::F32 => build::<f32>(&device, &config, channels, buffer.clone(), max_samples, on_level.clone()),
        SampleFormat::I16 => build::<i16>(&device, &config, channels, buffer.clone(), max_samples, on_level.clone()),
        SampleFormat::I32 => build::<i32>(&device, &config, channels, buffer.clone(), max_samples, on_level.clone()),
        SampleFormat::U16 => build::<u16>(&device, &config, channels, buffer.clone(), max_samples, on_level.clone()),
        other => return Err(mic_error(format!("unsupported sample format {other:?}"))),
    }?;
    stream.play().map_err(mic_error)?;
    let _ = ready.send(Ok(()));
    let started = Instant::now();
    loop {
        match stop.recv_timeout(Duration::from_millis(100)) {
            Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) if started.elapsed().as_secs_f32() > MAX_SECONDS + 1.0 => break,
            Err(_) => {}
        }
    }
    drop(stream);
    let samples = std::mem::take(&mut *buffer.lock().unwrap());
    Ok(Recording { samples, sample_rate })
}

fn build<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    channels: usize,
    buffer: Arc<Mutex<Vec<f32>>>,
    max_samples: usize,
    on_level: Arc<impl Fn(f32) + Send + Sync + 'static>,
) -> Result<cpal::Stream>
where
    T: SizedSample + Sample + Send + 'static,
    f32: FromSample<T>,
{
    let mut last_level = Instant::now();
    let mut level_acc: Vec<f32> = Vec::new();
    device
        .build_input_stream::<T, _, _>(
            *config,
            move |data: &[T], _| {
                let mut buf = buffer.lock().unwrap();
                for frame in data.chunks(channels) {
                    if buf.len() >= max_samples {
                        break;
                    }
                    let mono = frame.iter().map(|s| f32::from_sample(*s)).sum::<f32>() / channels as f32;
                    buf.push(mono);
                    level_acc.push(mono);
                }
                if last_level.elapsed() >= Duration::from_millis(100) {
                    on_level((rms(&level_acc) * 4.0).min(1.0));
                    level_acc.clear();
                    last_level = Instant::now();
                }
            },
            |e| log::warn!("microphone error: {e}"),
            None,
        )
        .map_err(mic_error)
}

/// Downsample to 16 kHz (averaging, which also filters) and encode as WAV.
pub fn to_wav(rec: &Recording) -> Result<Vec<u8>> {
    let samples = resample(&rec.samples, rec.sample_rate, TARGET_RATE);
    let spec = hound::WavSpec { channels: 1, sample_rate: TARGET_RATE, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
    let mut cursor = std::io::Cursor::new(Vec::with_capacity(samples.len() * 2 + 44));
    {
        let mut writer = hound::WavWriter::new(&mut cursor, spec).map_err(|e| Error::other(e.to_string()))?;
        for s in samples {
            writer.write_sample((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16).map_err(|e| Error::other(e.to_string()))?;
        }
        writer.finalize().map_err(|e| Error::other(e.to_string()))?;
    }
    Ok(cursor.into_inner())
}

fn resample(samples: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || from == 0 || samples.is_empty() {
        return samples.to_vec();
    }
    let ratio = from as f64 / to as f64;
    let out_len = (samples.len() as f64 / ratio).floor() as usize;
    (0..out_len)
        .map(|i| {
            let start = (i as f64 * ratio) as usize;
            let end = (((i + 1) as f64 * ratio) as usize).clamp(start + 1, samples.len());
            samples[start..end].iter().sum::<f32>() / (end - start) as f32
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resamples_and_encodes_a_wav() {
        // One second of a 440 Hz tone at 48 kHz.
        let samples: Vec<f32> = (0..48_000).map(|i| (i as f32 * 440.0 * std::f32::consts::TAU / 48_000.0).sin() * 0.5).collect();
        let rec = Recording { samples, sample_rate: 48_000 };
        assert!((rec.seconds() - 1.0).abs() < 1e-6);
        assert!(rec.peak_level() > 0.3);
        let wav = to_wav(&rec).unwrap();
        let reader = hound::WavReader::new(std::io::Cursor::new(&wav)).unwrap();
        assert_eq!(reader.spec().sample_rate, 16_000);
        assert_eq!(reader.spec().channels, 1);
        assert_eq!(reader.len(), 16_000);
        assert_eq!(Recording { samples: vec![0.0; 100], sample_rate: 16_000 }.peak_level(), 0.0);
    }
}
