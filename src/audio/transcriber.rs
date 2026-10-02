// src/audio/transcriber.rs
use crate::audio::ffmpeg;
use crate::podman::client::{WhisperClient, WhisperSegment};
use anyhow::{anyhow, Context, Result};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

const CHUNK_DURATION_SECS: u32 = 60;

pub fn transcribe_audio<P, F>(
    wav_path: P,
    whisper_endpoint: &str,
    max_len: usize,
    cancel_token: Arc<AtomicBool>, // <-- Add cancellation token
    mut on_progress: F,
) -> Result<Vec<WhisperSegment>>
where
    P: AsRef<Path>,
    F: FnMut(f32, String) -> Result<()>, // <-- Returns Result to detect channel disconnect
{
    let wav_path = wav_path.as_ref();

    if cancel_token.load(Ordering::Relaxed) {
        return Err(anyhow!("Task cancelled before start"));
    }

    on_progress(0.05, "Splitting and converting audio with FFmpeg...".into())?;
    let processed = ffmpeg::split_and_convert_audio(wav_path, CHUNK_DURATION_SECS)
        .context("Audio conversion failed")?;

    let total_chunks = processed.chunks.len();
    on_progress(
        0.10,
        format!("Split audio into {} chunk(s). Connecting to Whisper...", total_chunks),
    )?;

    let whisper = WhisperClient::new(whisper_endpoint);
    let mut all_segments = Vec::new();

    for (i, chunk) in processed.chunks.iter().enumerate() {
        // Check for cancellation between chunks
        if cancel_token.load(Ordering::Relaxed) {
            return Err(anyhow!("Task cancelled by user"));
        }

        let chunk_num = i + 1;
        let progress = 0.10 + (0.75 * (i as f32 / total_chunks as f32));

        on_progress(
            progress,
            format!("Transcribing chunk {}/{} (offset {:.1}s)...", chunk_num, total_chunks, chunk.start_offset_secs),
        )?;

        let mut segments = whisper
            .send_chunk(&chunk.path, max_len)
            .with_context(|| format!("Inference failed on chunk {}", chunk_num))?;

        for seg in &mut segments {
            seg.start += chunk.start_offset_secs;
            seg.end += chunk.start_offset_secs;
        }

        all_segments.extend(segments);
    }

    on_progress(0.85, "All chunks transcribed successfully.".into())?;
    Ok(all_segments)
}