// src/audio/ffmpeg.rs
use anyhow::{Context, Result, anyhow};
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

pub struct AudioChunk {
    pub path: PathBuf,
    /// Offset in seconds from the beginning of the original file
    pub start_offset_secs: f64,
}

pub struct ProcessedAudio {
    /// Keeps the temp directory alive until dropped
    pub _temp_dir: TempDir,
    pub chunks: Vec<AudioChunk>,
}

/// Converts a WAV file to 16kHz mono pcm_s16le and splits it into chunks.
/// If chunk_duration_secs is 0, the audio is not split and processed as a single chunk.
pub fn split_and_convert_audio<P: AsRef<Path>>(
    input: P,
    chunk_duration_secs: u32,
) -> Result<ProcessedAudio> {
    let input = input.as_ref();

    if !input.exists() {
        return Err(anyhow!(
            "Input audio file does not exist: {}",
            input.display()
        ));
    }

    let temp_dir = tempfile::Builder::new()
        .prefix("whisper_chunks_")
        .tempdir()
        .context("Failed to create temporary directory for chunks")?;

    // If chunk size is 0, convert entire file without segment muxing
    if chunk_duration_secs == 0 {
        let single_output = temp_dir.path().join("chunk_0000.wav");
        let output = Command::new("ffmpeg")
            .arg("-nostdin")
            .arg("-loglevel")
            .arg("error")
            .arg("-y")
            .arg("-threads")
            .arg("0")
            .arg("-i")
            .arg(input)
            .arg("-vn")
            .arg("-ar")
            .arg("16000")
            .arg("-ac")
            .arg("1")
            .arg("-c:a")
            .arg("pcm_s16le")
            .arg(&single_output)
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

        return Ok(ProcessedAudio {
            _temp_dir: temp_dir,
            chunks: vec![AudioChunk {
                path: single_output,
                start_offset_secs: 0.0,
            }],
        });
    }

    let output_pattern = temp_dir.path().join("chunk_%04d.wav");

    // FFmpeg command to resample and segment
    let output = Command::new("ffmpeg")
        .arg("-nostdin")
        .arg("-loglevel")
        .arg("error")
        .arg("-y")
        .arg("-threads")
        .arg("0")
        .arg("-i")
        .arg(input)
        .arg("-vn")
        .arg("-ar")
        .arg("16000")
        .arg("-ac")
        .arg("1")
        .arg("-c:a")
        .arg("pcm_s16le")
        .arg("-f")
        .arg("segment")
        .arg("-segment_time")
        .arg(chunk_duration_secs.to_string())
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
