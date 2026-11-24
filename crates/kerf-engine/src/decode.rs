//! Reads an audio file into mono samples at the file's own rate. Pure Rust
//! (symphonia): WAV, AIFF, FLAC, MP3 and Ogg Vorbis.

use std::fs::File;
use std::io::ErrorKind;
use std::path::Path;

use symphonia::core::audio::sample::Sample;
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::errors::Error;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

use crate::samples::SampleError;

pub(crate) struct Decoded {
    /// Channels averaged.
    pub mono: Vec<f32>,
    pub rate: u32,
}

/// Decodes up to `max_seconds` of the file.
pub(crate) fn decode(path: &Path, max_seconds: f32) -> Result<Decoded, SampleError> {
    let file = File::open(path).map_err(|e| match e.kind() {
        ErrorKind::NotFound => SampleError::NotFound,
        _ => SampleError::Io(e.to_string()),
    })?;
    let stream = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let unsupported = |e: Error| SampleError::Unsupported(e.to_string());
    let mut format = symphonia::default::get_probe()
        .probe(
            &hint,
            stream,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(unsupported)?;
    let track = format
        .default_track(TrackType::Audio)
        .ok_or(SampleError::Empty)?;
    let params = track
        .codec_params
        .as_ref()
        .and_then(|p| p.audio())
        .ok_or(SampleError::Empty)?;
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(params, &AudioDecoderOptions::default())
        .map_err(unsupported)?;
    let track_id = track.id;

    let mut mono = Vec::new();
    let mut scratch: Vec<f32> = Vec::new();
    let mut rate = 0;
    let mut cap = usize::MAX;
    while mono.len() < cap {
        let packet = match format.next_packet() {
            Ok(Some(p)) => p,
            Ok(None) => break,
            Err(_) if !mono.is_empty() => break,
            Err(e) => return Err(unsupported(e)),
        };
        if packet.track_id != track_id {
            continue;
        }
        let buf = match decoder.decode(&packet) {
            Ok(b) => b,
            Err(Error::DecodeError(_)) => continue,
            Err(_) if !mono.is_empty() => break,
            Err(e) => return Err(unsupported(e)),
        };
        let spec = buf.spec();
        let channels = spec.channels().count().max(1);
        if rate == 0 {
            rate = spec.rate();
            cap = (max_seconds * rate as f32) as usize;
        }
        scratch.resize(buf.samples_interleaved(), f32::MID);
        buf.copy_to_slice_interleaved(&mut scratch);
        mono.extend(
            scratch
                .chunks_exact(channels)
                .map(|f| f.iter().sum::<f32>() / channels as f32),
        );
    }
    mono.truncate(cap);
    if mono.is_empty() || rate == 0 {
        return Err(SampleError::Empty);
    }
    Ok(Decoded { mono, rate })
}
