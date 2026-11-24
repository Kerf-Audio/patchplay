//! Minimal WAV writer: 16-bit PCM, stereo.

use std::io;
use std::path::Path;

use crate::{Render, write_atomic};

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

impl Render {
    /// The render as a 16-bit stereo WAV file at `rate` Hz, in memory.
    pub fn wav_bytes(&self, rate: u32) -> io::Result<Vec<u8>> {
        let frames = self.left.len().min(self.right.len());
        let data_len = frames
            .checked_mul(4)
            .and_then(|n| u32::try_from(n).ok())
            .filter(|n| *n <= u32::MAX - 36)
            .ok_or_else(|| invalid("render too long for WAV"))?;
        let byte_rate = rate
            .checked_mul(4)
            .ok_or_else(|| invalid("sample rate too high for WAV"))?;
        let mut w = Vec::with_capacity(44 + frames * 4);
        w.extend_from_slice(b"RIFF");
        w.extend_from_slice(&(36 + data_len).to_le_bytes());
        w.extend_from_slice(b"WAVEfmt ");
        w.extend_from_slice(&16u32.to_le_bytes());
        w.extend_from_slice(&1u16.to_le_bytes()); // PCM
        w.extend_from_slice(&2u16.to_le_bytes()); // channels
        w.extend_from_slice(&rate.to_le_bytes());
        w.extend_from_slice(&byte_rate.to_le_bytes());
        w.extend_from_slice(&4u16.to_le_bytes()); // bytes per frame
        w.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
        w.extend_from_slice(b"data");
        w.extend_from_slice(&data_len.to_le_bytes());
        for (&l, &r) in self.left.iter().zip(&self.right) {
            for x in [l, r] {
                let s = (x.clamp(-1.0, 1.0) * f32::from(i16::MAX)).round() as i16;
                w.extend_from_slice(&s.to_le_bytes());
            }
        }
        Ok(w)
    }

    /// Writes the render as a 16-bit stereo WAV at `rate` Hz. An existing file
    /// is replaced whole or not at all.
    pub fn write_wav(&self, path: &Path, rate: u32) -> io::Result<()> {
        write_atomic(path, &self.wav_bytes(rate)?)
    }
}

#[cfg(test)]
mod tests {
    use crate::Render;

    #[test]
    fn a_sample_rate_that_overflows_the_byte_rate_is_an_error_not_a_panic() {
        let render = Render {
            left: vec![0.0; 4],
            right: vec![0.0; 4],
        };
        let e = render.wav_bytes(u32::MAX / 2).unwrap_err();
        assert_eq!(e.kind(), std::io::ErrorKind::InvalidInput);
        let wav = render.wav_bytes(48_000).unwrap();
        assert_eq!(wav.len(), 44 + 16);
        assert_eq!(u32::from_le_bytes(wav[28..32].try_into().unwrap()), 192_000);
    }
}
