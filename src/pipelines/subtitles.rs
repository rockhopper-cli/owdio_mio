use anyhow::{Result, anyhow};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;

use crate::audio::transcriber;
use crate::pipelines::PipelineMessage;
use crate::subtitles::formatter;

#[derive(Debug, Clone)]
pub struct SubtitleOptions {
    pub chunk_duration_secs: u32,
    pub max_chars_per_line: usize,
    pub two_lines: bool,
    pub min_gap_ms: u64,
    pub spellcheck: bool,
    pub qwen_port: u16,
}

#[derive(Debug, Clone, Default)]
pub struct SubtitlePipelineResult {
    pub raw_srt: String,
    pub raw_srt_path: String,
    pub proofread_srt: Option<String>,
    pub proofread_srt_path: Option<String>,
}

fn save_with_fallback(primary_path: &Path, content: &str) -> (PathBuf, Option<String>) {
    if let Err(e) = std::fs::write(primary_path, content) {
        let fallback = dirs::document_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(primary_path.file_name().unwrap_or_default());
        let _ = std::fs::write(&fallback, content);
        (
            fallback,
            Some(format!(
                "Could not save to {}: {}. Saved to fallback location.",
                primary_path.display(),
                e
            )),
        )
    } else {
        (primary_path.to_path_buf(), None)
    }
}

pub fn generate_subtitles<P: AsRef<Path>>(
    wav_path: P,
    whisper_endpoint: &str,
    options: SubtitleOptions,
    cancel_token: Arc<AtomicBool>,
    tx: Sender<PipelineMessage<SubtitlePipelineResult>>,
) -> Result<SubtitlePipelineResult> {
    let wav_path = wav_path.as_ref();

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

    let _ = tx.send(PipelineMessage::Progress {
        progress: 0.82,
        text: "Formatting subtitles & adjusting gaps...".into(),
    });

    let subtitles = formatter::format(
        &segments,
        options.max_chars_per_line,
        options.two_lines,
        options.min_gap_ms,
    )?;
    let raw_srt = formatter::to_srt(&subtitles);

    let (saved_path, warning) = save_with_fallback(&wav_path.with_extension("srt"), &raw_srt);
    if let Some(w) = warning {
        let _ = tx.send(PipelineMessage::Log(format!("[Warning] {}", w)));
    } else {
        let _ = tx.send(PipelineMessage::Log(format!(
            "Saved raw subtitles to: {}",
            saved_path.display()
        )));
    }

    let mut proofread_srt = None;
    let mut proofread_srt_path = None;

    if options.spellcheck && !cancel_token.load(Ordering::Relaxed) {
        let _ = tx.send(PipelineMessage::Log(
            "[Subtitles] Starting AI spell check & proofreading with Qwen...".into(),
        ));

        let llm = crate::clients::llm::LlmClient::new(options.qwen_port);
        let tx_progress = tx.clone();

        match llm.spellcheck_srt(&raw_srt, move |batch, total, msg| {
            let prog = 0.85 + (0.13 * (batch as f32 / total as f32));
            let _ = tx_progress.send(PipelineMessage::Progress {
                progress: prog,
                text: format!("AI Spell Check ({}/{})...", batch, total),
            });
            let _ = tx_progress.send(PipelineMessage::Log(format!("[AI Spell Check] {}", msg)));
        }) {
            Ok(corrected) => {
                let (p_path, p_warn) =
                    save_with_fallback(&wav_path.with_extension("proofread.srt"), &corrected);
                if let Some(w) = p_warn {
                    let _ = tx.send(PipelineMessage::Log(format!("[Warning] {}", w)));
                } else {
                    let _ = tx.send(PipelineMessage::Log(format!(
                        "Saved proofread subtitles to: {}",
                        p_path.display()
                    )));
                }
                proofread_srt_path = Some(p_path.display().to_string());
                proofread_srt = Some(corrected);
            }
            Err(e) => {
                let _ = tx.send(PipelineMessage::Log(format!(
                    "Warning: Subtitle spell check failed: {:#}",
                    e
                )));
            }
        }
    }

    Ok(SubtitlePipelineResult {
        raw_srt,
        raw_srt_path: saved_path.display().to_string(),
        proofread_srt,
        proofread_srt_path,
    })
}
