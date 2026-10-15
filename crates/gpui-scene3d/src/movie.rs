//! Movie export: PNG sequences and MP4 muxing.
//!
//! The portable artifact is a numbered PNG sequence: it works on every
//! host, including wasm, where each frame can be downloaded or fed to a
//! browser encoder. On native hosts the `mp4` feature additionally muxes
//! frames to H.264 MP4 through the system `ffmpeg`, located at runtime.

// Rust guideline compliant 2026-02-21

use crate::Frame;

/// Failures reported by movie export.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum MovieError {
    /// Frame timestamps must be strictly increasing.
    #[error("movie frames must have strictly increasing times")]
    NonIncreasingTimes,
    /// The requested encoder is unavailable or failed.
    #[error("encoder failed: {0}")]
    Encoder(String),
    /// A frame could not be encoded as PNG.
    #[error("png encoding failed: {0}")]
    Png(String),
}

/// One frame of a movie: physical time plus its rendered image.
pub type TimedFrame = (f64, Frame);

/// Validates frame timing shared by every export path.
///
/// # Errors
///
/// Returns [`MovieError::NonIncreasingTimes`] for empty or unordered input.
pub fn validate_frames(frames: &[TimedFrame]) -> Result<(), MovieError> {
    if frames.is_empty() || frames.windows(2).any(|pair| pair[1].0 <= pair[0].0) {
        return Err(MovieError::NonIncreasingTimes);
    }
    Ok(())
}

/// Frame rate implied by frame times, for encoders that need a constant rate.
#[must_use]
pub fn frame_rate(frames: &[TimedFrame]) -> f64 {
    if frames.len() < 2 {
        return 30.0;
    }
    let span = frames[frames.len() - 1].0 - frames[0].0;
    if span <= 0.0 {
        return 30.0;
    }
    (frames.len() - 1) as f64 / span
}

/// Encodes frames as a numbered PNG sequence: `frame_000000.png`, and so on.
///
/// Available under the `png` feature. File names sort lexicographically in
/// playback order, so any standard muxer or image viewer plays them back.
///
/// # Errors
///
/// Returns an error for unordered times or a PNG encoding failure.
#[cfg(feature = "png")]
pub fn png_sequence(frames: &[TimedFrame]) -> Result<Vec<(String, Vec<u8>)>, MovieError> {
    validate_frames(frames)?;
    let mut out = Vec::with_capacity(frames.len());
    for (index, (_, frame)) in frames.iter().enumerate() {
        let bytes = frame.to_png().map_err(MovieError::Png)?;
        out.push((format!("frame_{index:06}.png"), bytes));
    }
    Ok(out)
}

/// Muxes frames to an H.264 MP4 (yuv420p, fragmented for streaming).
///
/// Frames are piped to `ffmpeg` as a PNG stream at the rate implied by
/// their times, and the MP4 bytes are returned in memory; nothing touches
/// the filesystem. Odd frame dimensions are padded to even sizes, as
/// yuv420p requires.
///
/// # Errors
///
/// Returns [`MovieError::Encoder`] when `ffmpeg` is missing or fails. This
/// function is unavailable on wasm, where [`png_sequence`] is the export.
#[cfg(all(feature = "mp4", not(target_family = "wasm")))]
pub fn mux_mp4(frames: &[TimedFrame]) -> Result<Vec<u8>, MovieError> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    validate_frames(frames)?;
    let fps = frame_rate(frames);
    let mut child = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "image2pipe",
            "-framerate",
            &format!("{fps}"),
            "-i",
            "pipe:0",
            "-vf",
            "pad=ceil(iw/2)*2:ceil(ih/2)*2",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-movflags",
            "frag_keyframe+empty_moov",
            "-f",
            "mp4",
            "pipe:1",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| MovieError::Encoder(format!("cannot start ffmpeg: {e}")))?;
    {
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| MovieError::Encoder("ffmpeg stdin unavailable".into()))?;
        for (_, frame) in frames {
            let png = frame.to_png().map_err(MovieError::Png)?;
            stdin
                .write_all(&png)
                .map_err(|e| MovieError::Encoder(format!("cannot feed ffmpeg: {e}")))?;
        }
    }
    let output = child
        .wait_with_output()
        .map_err(|e| MovieError::Encoder(format!("ffmpeg failed to finish: {e}")))?;
    if !output.status.success() {
        return Err(MovieError::Encoder(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    if output.stdout.len() < 8 || &output.stdout[4..8] != b"ftyp" {
        return Err(MovieError::Encoder(
            "ffmpeg did not produce an MP4 stream".into(),
        ));
    }
    Ok(output.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(value: u8) -> Frame {
        Frame {
            width: 16,
            height: 16,
            rgba: vec![value, value, value, 255]
                .into_iter()
                .cycle()
                .take(16 * 16 * 4)
                .collect(),
        }
    }

    #[test]
    fn frame_rate_comes_from_times() {
        let frames = vec![(0.0, frame(0)), (0.5, frame(1)), (1.0, frame(2))];
        assert_eq!(frame_rate(&frames), 2.0);
        assert_eq!(validate_frames(&frames), Ok(()));
        assert_eq!(
            validate_frames(&[(1.0, frame(0)), (0.5, frame(1))]),
            Err(MovieError::NonIncreasingTimes)
        );
    }

    #[cfg(feature = "png")]
    #[test]
    fn png_sequence_names_sort_in_playback_order() {
        let frames = vec![(0.0, frame(0)), (0.5, frame(90)), (1.0, frame(180))];
        let sequence = png_sequence(&frames).unwrap();
        assert_eq!(sequence.len(), 3);
        assert_eq!(sequence[0].0, "frame_000000.png");
        assert_eq!(sequence[2].0, "frame_000002.png");
        assert!(sequence.iter().all(|(_, bytes)| &bytes[..4] == b"\x89PNG"));
    }

    #[cfg(all(feature = "mp4", not(target_family = "wasm")))]
    #[test]
    fn muxes_a_real_mp4_when_ffmpeg_is_available() {
        let frames = vec![(0.0, frame(10)), (0.25, frame(90)), (0.5, frame(170))];
        match mux_mp4(&frames) {
            Ok(bytes) => {
                assert!(&bytes[4..8] == b"ftyp", "missing MP4 signature");
                assert!(bytes.len() > 200, "suspiciously small MP4: {}", bytes.len());
            }
            Err(MovieError::Encoder(message)) if message.contains("cannot start ffmpeg") => {
                // The build host has no ffmpeg; PNG sequence remains the path.
            }
            Err(other) => panic!("unexpected mux failure: {other}"),
        }
    }
}
