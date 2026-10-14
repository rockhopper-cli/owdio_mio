// src/subtitle_pipeline.rs
use anyhow::Result;
use anyhow::anyhow;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;

use crate::audio::transcriber;
use crate::audio_pipeline::PipelineMessage;
use crate::subtitles::formatter;

#[derive(Debug, Clone, Copy)]
pub struct SubtitleOptions {
    pub chunk_duration_secs: u32,
    pub max_chars_per_line: usize,
    pub two_lines: bool,
    pub min_gap_ms: u64,
}

pub fn generate_subtitles<P: AsRef<Path>>(
    wav_path: P,
    whisper_endpoint: &str,
    options: SubtitleOptions,
    cancel_token: Arc<AtomicBool>,
    tx: Sender<PipelineMessage<String>>,
) -> Result<String> {
    let wav_path = wav_path.as_ref();

    // 1. Run core transcription engine
    let tx_clone = tx.clone();
    let segments = transcriber::transcribe_audio(
        wav_path,
        whisper_endpoint,
        options.chunk_duration_secs,
        options.max_chars_per_line,
        cancel_token.clone(),
        move |progress, text| {
            tx_clone
                .send(PipelineMessage::Progress {
                    progress,
                    text: text.clone(),
                })
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
    let _ = tx.send(PipelineMessage::Log(
        "Formatting subtitles & adjusting gaps...".into(),
    ));

    let subtitles = formatter::format(
        &segments,
        options.max_chars_per_line,
        options.two_lines,
        options.min_gap_ms,
    )?;
    let srt = formatter::to_srt(&subtitles);

    Ok(srt)
}
