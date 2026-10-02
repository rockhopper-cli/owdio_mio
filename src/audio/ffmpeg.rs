// src/audio/ffmpeg.rs

use anyhow::{anyhow, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

pub struct AudioChunk {
    pub path: PathBuf,
    /// Offset in seconds from the beginning of the original file (e.g., chunk 1 starts at 120.0s)
    pub start_offset_secs: f64,
}

pub struct ProcessedAudio {
    /// Keeps the temp directory alive until dropped
    pub _temp_dir: TempDir,
    pub chunks: Vec<AudioChunk>,
}

/// Converts a WAV file to 16kHz mono pcm_s16le and splits it into 120-second chunks.
pub fn split_and_convert_audio<P: AsRef<Path>>(
    input: P,
    chunk_duration_secs: u32,
) -> Result<ProcessedAudio> {
    let input = input.as_ref();

    if !input.exists() {
        return Err(anyhow!("Input audio file does not exist: {}", input.display()));
    }

    // Temporary directory where 2-minute chunks will live
    let temp_dir = tempfile::Builder::new()
        .prefix("whisper_chunks_")
        .tempdir()
        .context("Failed to create temporary directory for chunks")?;

    let output_pattern = temp_dir.path().join("chunk_%04d.wav");

    // FFmpeg command to resample and segment
    let output = Command::new("ffmpeg")
        .arg("-nostdin") // Disables standard input interaction
        .arg("-loglevel")
        .arg("error") // Suppresses frame-by-frame stderr progress text
        .arg("-y") // Overwrite output files if needed
        .arg("-threads")
        .arg("0") // Use all available CPU cores for decoding/resampling
        .arg("-i")
        .arg(input)
        .arg("-vn") // Disables video stream processing
        .arg("-ar")
        .arg("16000") // 16 kHz sample rate
        .arg("-ac")
        .arg("1") // Mono
        .arg("-c:a")
        .arg("pcm_s16le") // 16-bit PCM
        .arg("-f")
        .arg("segment") // Segment muxer
        .arg("-segment_time")
        .arg(chunk_duration_secs.to_string()) // 60 seconds
        .arg("-reset_timestamps")
        .arg("1")
        .arg(output_pattern)
        .output()
        .context("Failed to execute ffmpeg. Ensure ffmpeg is installed and on your PATH.")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!(
            "FFmpeg failed with exit code {:?}:\n{}",
            output.status.code(),
            stderr
        ));
    }

    // Collect all generated chunks in sorted order
    let mut chunk_paths: Vec<PathBuf> = std::fs::read_dir(temp_dir.path())
        .context("Failed to read chunk temp directory")?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| p.extension().map_or(false, |ext| ext == "wav"))
        .collect();

    chunk_paths.sort();

    if chunk_paths.is_empty() {
        return Err(anyhow!("FFmpeg produced no audio chunks"));
    }

    let chunks = chunk_paths
        .into_iter()
        .enumerate()
        .map(|(index, path)| AudioChunk {
            path,
            start_offset_secs: (index as f64) * (chunk_duration_secs as f64),
        })
        .collect();

    Ok(ProcessedAudio {
        _temp_dir: temp_dir,
        chunks,
    })
}