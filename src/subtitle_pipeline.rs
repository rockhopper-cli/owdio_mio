// src/subtitle_pipeline.rs
use anyhow::Result;
use std::path::Path;
use std::sync::mpsc::Sender;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use anyhow::anyhow;

use crate::audio::transcriber;
use crate::subtitles::formatter;
use crate::audio_pipeline::PipelineMessage; 


pub fn generate_subtitles<P: AsRef<Path>>(
    wav_path: P,
    whisper_endpoint: &str,
    max_chars_per_line: usize,
    two_lines: bool,
    min_gap_ms: u64,
    cancel_token: Arc<AtomicBool>, // <-- Add this
    tx: Sender<PipelineMessage<String>>,
) -> Result<String> {
    let wav_path = wav_path.as_ref();

    // 1. Run core transcription engine
    let tx_clone = tx.clone();
    let segments = transcriber::transcribe_audio(
        wav_path,
        whisper_endpoint,
        max_chars_per_line,
        cancel_token.clone(),
        move |progress, text| {
            tx_clone
                .send(PipelineMessage::Progress { progress, text: text.clone() })
                .map_err(|_| anyhow!("UI receiver disconnected"))?;
            let _ = tx_clone.send(PipelineMessage::Log(text));
            Ok(())
        },
    )?;

    if cancel_token.load(Ordering::Relaxed) {
        return Err(anyhow!("Task cancelled by user"));
    }

    // 2. Subtitle formatting stage (0.85 -> 1.0)
    let _ = tx.send(PipelineMessage::Progress {
        progress: 0.90,
        text: "Formatting subtitles & adjusting gaps...".into(),
    });
    let _ = tx.send(PipelineMessage::Log("Formatting subtitles & adjusting gaps...".into()));

    let subtitles = formatter::format(&segments, max_chars_per_line, two_lines, min_gap_ms)?;
    let srt = formatter::to_srt(&subtitles);

    Ok(srt)
}