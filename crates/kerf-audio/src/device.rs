//! The cpal edge: opens the default output device and runs a `Voice` in its
//! callback.
//!
//! A cpal `Stream` is not `Send` on every platform, so a thread owns it for
//! the player's whole life. The same thread empties the queue of retired
//! engines, so they never drop on the audio thread.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use std::time::Instant;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{ErrorKind, FromSample, Sample, SampleFormat, SizedSample, Stream, StreamConfig};
use kerf_engine::Engine;
use rtrb::{Consumer, Producer, RingBuffer};

use crate::AudioError;
use crate::voice::{Msg, Peaks, Voice};

/// Frames rendered per `Voice::render` call; bigger device buffers are split.
const BLOCK_FRAMES: usize = 1024;
const RETIRED_CAPACITY: usize = 64;
const DRAIN_INTERVAL: Duration = Duration::from_millis(50);

pub(crate) struct Device {
    pub(crate) sample_rate: u32,
    shutdown: Option<Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl Device {
    /// Opens the default output device and starts the stream, which plays
    /// silence until the voice gets an engine.
    pub(crate) fn open(
        messages: Consumer<Msg>,
        peaks: Arc<Peaks>,
        lost: Arc<AtomicBool>,
    ) -> Result<Self, AudioError> {
        let (ready_tx, ready_rx) = mpsc::channel();
        let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>();
        let thread = thread::Builder::new()
            .name("kerf-audio".into())
            .spawn(move || {
                let (retired_tx, mut retired_rx) = RingBuffer::new(RETIRED_CAPACITY);
                let stream = match start(messages, retired_tx, peaks, lost) {
                    Ok((stream, rate)) => {
                        let _ = ready_tx.send(Ok(rate));
                        stream
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                while let Err(RecvTimeoutError::Timeout) = shutdown_rx.recv_timeout(DRAIN_INTERVAL)
                {
                    while retired_rx.pop().is_ok() {}
                }
                drop(stream);
            })
            .map_err(|e| AudioError::Device(e.to_string()))?;
        let sample_rate = ready_rx
            .recv()
            .map_err(|_| AudioError::Device("the audio thread stopped while starting".into()))??;
        Ok(Self {
            sample_rate,
            shutdown: Some(shutdown_tx),
            thread: Some(thread),
        })
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        drop(self.shutdown.take());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn start(
    messages: Consumer<Msg>,
    retired: Producer<Box<Engine>>,
    peaks: Arc<Peaks>,
    lost: Arc<AtomicBool>,
) -> Result<(Stream, u32), AudioError> {
    let device = cpal::default_host()
        .default_output_device()
        .ok_or(AudioError::NoDevice)?;
    let supported = device.default_output_config().map_err(device_error)?;
    let config: StreamConfig = supported.into();
    let voice = Voice::new(messages, retired, Arc::clone(&peaks), config.sample_rate);
    let stream = match supported.sample_format() {
        SampleFormat::F32 => build::<f32>(&device, config, voice, peaks, lost),
        SampleFormat::I16 => build::<i16>(&device, config, voice, peaks, lost),
        SampleFormat::U16 => build::<u16>(&device, config, voice, peaks, lost),
        SampleFormat::I32 => build::<i32>(&device, config, voice, peaks, lost),
        other => Err(AudioError::Device(format!(
            "the output device wants {other} samples, which patchplay does not support"
        ))),
    }?;
    stream.play().map_err(device_error)?;
    Ok((stream, config.sample_rate))
}

fn build<T>(
    device: &cpal::Device,
    config: StreamConfig,
    mut voice: Voice,
    peaks: Arc<Peaks>,
    lost: Arc<AtomicBool>,
) -> Result<Stream, AudioError>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = usize::from(config.channels).max(1);
    let mut stereo = vec![0.0; 2 * BLOCK_FRAMES];
    let rate = config.sample_rate;
    device
        .build_output_stream(
            config,
            move |data: &mut [T], _| {
                let started = Instant::now();
                for block in data.chunks_mut(channels * BLOCK_FRAMES) {
                    let stereo = &mut stereo[..2 * (block.len() / channels)];
                    voice.render(stereo);
                    interleave(stereo, block, channels);
                }
                let busy = started.elapsed().as_nanos() as u64;
                peaks.load.record(busy, data.len() / channels, rate);
            },
            move |err| {
                if matches!(
                    err.kind(),
                    ErrorKind::DeviceNotAvailable | ErrorKind::StreamInvalidated
                ) {
                    lost.store(true, Ordering::Relaxed);
                }
            },
            None,
        )
        .map_err(device_error)
}

fn device_error(e: cpal::Error) -> AudioError {
    match e.kind() {
        ErrorKind::DeviceNotAvailable => AudioError::NoDevice,
        _ => AudioError::Device(e.to_string()),
    }
}

/// Spreads stereo frames over the device's channels: a mono device gets the
/// mix, wider ones get left and right on the first two and silence elsewhere.
fn interleave<T>(stereo: &[f32], out: &mut [T], channels: usize)
where
    T: Sample + FromSample<f32>,
{
    for (frame, lr) in out.chunks_exact_mut(channels).zip(stereo.chunks_exact(2)) {
        match frame {
            [mono] => *mono = T::from_sample((lr[0] + lr[1]) * 0.5),
            [l, r, rest @ ..] => {
                *l = T::from_sample(lr[0]);
                *r = T::from_sample(lr[1]);
                rest.fill(T::EQUILIBRIUM);
            }
            [] => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::interleave;

    #[test]
    fn mono_devices_get_the_mix() {
        let mut out = [0.0f32; 2];
        interleave(&[0.5, -0.1, 0.2, 0.2], &mut out, 1);
        assert_eq!(out, [0.2, 0.2]);
    }

    #[test]
    fn stereo_devices_get_both_channels() {
        let mut out = [0.0f32; 4];
        interleave(&[0.5, -0.1, 0.2, 0.3], &mut out, 2);
        assert_eq!(out, [0.5, -0.1, 0.2, 0.3]);
    }

    #[test]
    fn extra_channels_are_silent_and_integer_formats_convert() {
        let mut out = [7i16; 6];
        interleave(&[1.0, -1.0, 0.0, 0.0], &mut out, 3);
        assert_eq!(out[2], 0);
        assert_eq!(out[5], 0);
        assert!(out[0] > 32_000 && out[1] < -32_000);
        assert_eq!(&out[3..5], [0, 0]);
    }
}
