use crate::audio::transcriber;
use crate::clients::llm::LlmClient;
use crate::pipelines::PipelineMessage;
use crate::utils::format_transcript_timestamp;

use anyhow::{Result, anyhow};
use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;

#[derive(Clone, Debug)]
pub struct PipelineResult {
    pub transcript_path: String,
    pub transcript_text: String,
    pub proofread_path: Option<String>,
    pub proofread_text: Option<String>,
    pub summary_path: Option<String>,
    pub summary_text: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct LlmGenerationOptions {
    pub summary: bool,
    pub spellcheck: bool,
}

fn save_with_fallback(primary_path: &Path, content: &str) -> (PathBuf, Option<String>) {
    if let Err(e) = fs::write(primary_path, content) {
        let fallback = dirs::document_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(primary_path.file_name().unwrap_or_default());
        let _ = fs::write(&fallback, content);
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

pub fn run_audio_pipeline(
    wav_path: String,
    whisper_port: u16,
    qwen_port: u16,
    chunk_duration_secs: u32,
    options: LlmGenerationOptions,
    cancel_token: Arc<AtomicBool>,
    tx: Sender<PipelineMessage<PipelineResult>>,
) -> Result<PipelineResult, String> {
    let input_path = Path::new(&wav_path);
    let whisper_url = format!("http://127.0.0.1:{}/inference", whisper_port);

    let tx_clone = tx.clone();
    let segments = transcriber::transcribe_audio(
        input_path,
        &whisper_url,
        chunk_duration_secs,
        0,
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
    )
    .map_err(|e| format!("{:#}", e))?;

    if cancel_token.load(Ordering::Relaxed) {
        return Err("Task cancelled by user".to_string());
    }

    let mut full_transcript = String::with_capacity(segments.len() * 80);
    for seg in &segments {
        let _ = writeln!(
            full_transcript,
            "[{}] {}",
            format_transcript_timestamp(seg.start),
            seg.text
        );
    }

    let (transcript_path, warn) = save_with_fallback(
        &input_path.with_extension("transcript.txt"),
        &full_transcript,
    );
    if let Some(w) = warn {
        let _ = tx.send(PipelineMessage::Log(format!("[Warning] {}", w)));
    } else {
        let _ = tx.send(PipelineMessage::Log(format!(
            "Saved transcript to: {}",
            transcript_path.display()
        )));
    }

    let llm = LlmClient::new(qwen_port);
    let mut proofread_path_out = None;
    let mut proofread_text_out = None;

    if options.spellcheck && !cancel_token.load(Ordering::Relaxed) {
        let _ = tx.send(PipelineMessage::Log(
            "[Transcription] Starting AI spell check and proofreading...".into(),
        ));

        let tx_progress = tx.clone();
        match llm.spellcheck(&full_transcript, move |batch, total, msg| {
            let prog = 0.85 + (0.09 * (batch as f32 / total as f32));
            let _ = tx_progress.send(PipelineMessage::Progress {
                progress: prog,
                text: format!("AI Spell Check ({}/{})...", batch, total),
            });
            let _ = tx_progress.send(PipelineMessage::Log(format!("[AI Spell Check] {}", msg)));
        }) {
            Ok(corrected) => {
                let (p_path, p_warn) =
                    save_with_fallback(&input_path.with_extension("proofread.txt"), &corrected);
                if let Some(w) = p_warn {
                    let _ = tx.send(PipelineMessage::Log(format!("[Warning] {}", w)));
                } else {
                    let _ = tx.send(PipelineMessage::Log(format!(
                        "Saved proofread transcript to: {}",
                        p_path.display()
                    )));
                }
                proofread_path_out = Some(p_path.display().to_string());
                proofread_text_out = Some(corrected);
            }
            Err(e) => {
                let _ = tx.send(PipelineMessage::Log(format!(
                    "Warning: AI Spell Check failed: {:#}",
                    e
                )));
            }
        }
    }

    let mut summary_path_out = None;
    let mut summary_text_out = None;

    if options.summary && !cancel_token.load(Ordering::Relaxed) {
        let _ = tx.send(PipelineMessage::Progress {
            progress: 0.94,
            text: "Generating Executive Summary with Qwen...".into(),
        });

        let prompt = "You are an expert transcriber and executive summarizer. \
                      Given the timestamped transcript, provide a concise high-level executive summary \
                      followed by organized bullet points highlighting key decisions, takeaways, and insights.";

        let source_for_summary = proofread_text_out.as_ref().unwrap_or(&full_transcript);

        match llm.complete(prompt, source_for_summary) {
            Ok(summary) => {
                let (s_path, s_warn) =
                    save_with_fallback(&input_path.with_extension("summary.txt"), &summary);
                if let Some(w) = s_warn {
                    let _ = tx.send(PipelineMessage::Log(format!("[Warning] {}", w)));
                } else {
                    let _ = tx.send(PipelineMessage::Log(format!(
                        "Saved summary to: {}",
                        s_path.display()
                    )));
                }
                summary_path_out = Some(s_path.display().to_string());
                summary_text_out = Some(summary);
            }
            Err(e) => {
                let _ = tx.send(PipelineMessage::Log(format!(
                    "Warning: Summary generation failed: {:#}",
                    e
                )));
            }
        }
    }

    Ok(PipelineResult {
        transcript_path: transcript_path.display().to_string(),
        transcript_text: full_transcript,
        proofread_path: proofread_path_out,
        proofread_text: proofread_text_out,
        summary_path: summary_path_out,
        summary_text: summary_text_out,
    })
}
