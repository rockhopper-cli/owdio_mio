// src/audio_pipeline.rs
use crate::audio::transcriber;
use crate::podman::llm_client::LlmClient;
use crate::utils::format_transcript_timestamp;  
use anyhow::Result;
use std::fs;
use std::path::Path;
use std::sync::mpsc::Sender;
use std::fmt::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use anyhow::anyhow;

#[derive(Clone, Debug)]
pub struct PipelineResult {
    pub transcript_path: String,
    pub transcript_text: String,
    pub summary_path: Option<String>,
    pub summary_text: Option<String>,
    pub youtube_path: Option<String>,
    pub youtube_text: Option<String>,
    pub patreon_path: Option<String>,
    pub patreon_text: Option<String>,
}

#[derive(Clone, Debug)]
pub enum PipelineMessage<T> {
    Log(String),
    Progress { progress: f32, text: String },
    Finished(Result<T, String>),
}

#[derive(Clone, Debug, Default)]
pub struct LlmGenerationOptions {
    pub summary: bool,
    pub youtube: bool,
    pub patreon: bool,
}

pub fn run_audio_pipeline(
    wav_path: String,
    whisper_port: u16,
    qwen_port: u16,
    options: LlmGenerationOptions,
    cancel_token: Arc<AtomicBool>, // <-- Add this
    tx: Sender<PipelineMessage<PipelineResult>>,
) -> Result<PipelineResult, String> {
    let input_path = Path::new(&wav_path);
    let whisper_url = format!("http://127.0.0.1:{}/inference", whisper_port);

    // 1. Run core transcription engine
    let tx_clone = tx.clone();
    let segments = transcriber::transcribe_audio(
        input_path,
        &whisper_url,
        0,
        cancel_token.clone(),
        move |progress, text| {
            tx_clone
                .send(PipelineMessage::Progress { progress, text: text.clone() })
                .map_err(|_| anyhow!("UI receiver disconnected"))?;
            let _ = tx_clone.send(PipelineMessage::Log(text));
            Ok(())
        },
    )
    .map_err(|e| format!("{:#}", e))?;

    if cancel_token.load(Ordering::Relaxed) {
        return Err("Task cancelled by user".to_string());
    }

    // 2. Format segments into timestamped text (Pre-allocated without per-line String allocations)
    let mut full_transcript = String::with_capacity(segments.len() * 80);
    for seg in &segments {
        let _ = writeln!(
            full_transcript,
            "[{}] {}",
            format_transcript_timestamp(seg.start),
            seg.text
        );
    }

    let transcript_path = input_path.with_extension("transcript.txt");
    fs::write(&transcript_path, &full_transcript)
        .map_err(|e| format!("Failed to save transcript file: {:#}", e))?;

    let _ = tx.send(PipelineMessage::Log(format!(
        "Saved transcript to: {}",
        transcript_path.display()
    )));

    // 3. Optional LLM stage with Qwen 2.5 Coder (0.85 -> 1.0)
    let llm = LlmClient::new(qwen_port);

    let mut summary_path_out = None;
    let mut summary_text_out = None;
    let mut youtube_path_out = None;
    let mut youtube_text_out = None;
    let mut patreon_path_out = None;
    let mut patreon_text_out = None;

    // 3A. Executive Summary
    if options.summary && !cancel_token.load(Ordering::Relaxed) {
        let _ = tx.send(PipelineMessage::Progress {
            progress: 0.88,
            text: "Generating Executive Summary with Qwen...".into(),
        });

        let prompt = "You are an expert transcriber and executive summarizer. \
                      Given the timestamped transcript, provide a concise high-level executive summary \
                      followed by organized bullet points highlighting key decisions and insights.";

        match llm.complete(prompt, &full_transcript) {
            Ok(summary) => {
                let p = input_path.with_extension("summary.txt");
                let _ = fs::write(&p, &summary);
                let _ = tx.send(PipelineMessage::Log(format!("Saved summary to: {}", p.display())));
                summary_path_out = Some(p.display().to_string());
                summary_text_out = Some(summary);
            }
            Err(e) => {
                let _ = tx.send(PipelineMessage::Log(format!("Warning: Summary generation failed: {:#}", e)));
            }
        }
    }

    // 3B. YouTube Description with Timestamps / Chapters
    if options.youtube && !cancel_token.load(Ordering::Relaxed) {
        let _ = tx.send(PipelineMessage::Progress {
            progress: 0.92,
            text: "Generating YouTube Description & Chapters with Qwen...".into(),
        });

        let prompt = "You are an expert YouTube producer and SEO copywriter. \
                      Given the transcript with timestamps, produce an engaging YouTube video description.\n\
                      Structure requirements:\n\
                      1. Hook & Overview: 2-3 engaging sentences describing the video.\n\
                      2. Key Takeaways: 3-5 bullet points of what viewers will learn.\n\
                      3. TIMESTAMPS / CHAPTERS:\n\
                         - Derive chapter markers from the timestamps provided in the transcript.\n\
                         - First chapter MUST start at '00:00 - Introduction' (or title).\n\
                         - Format strictly as 'MM:SS - Chapter Title' (or 'HH:MM:SS - Title').\n\
                      4. Call to Action: Reminder to like, subscribe, and comment.\n\
                      5. 5-8 relevant hashtags at the bottom.";

        match llm.complete(prompt, &full_transcript) {
            Ok(yt) => {
                let p = input_path.with_extension("youtube.txt");
                let _ = fs::write(&p, &yt);
                let _ = tx.send(PipelineMessage::Log(format!("Saved YouTube description to: {}", p.display())));
                youtube_path_out = Some(p.display().to_string());
                youtube_text_out = Some(yt);
            }
            Err(e) => {
                let _ = tx.send(PipelineMessage::Log(format!("Warning: YouTube generation failed: {:#}", e)));
            }
        }
    }

    // 3C. Patreon Community Post
    if options.patreon && !cancel_token.load(Ordering::Relaxed) {
        let _ = tx.send(PipelineMessage::Progress {
            progress: 0.96,
            text: "Generating Patreon Community Post with Qwen...".into(),
        });

        let prompt = "You are a creator writing directly to your patrons/supporters.\n\
                      Given the transcript, write a warm, engaging Patreon post.\n\
                      Include:\n\
                      1. A warm greeting and thank you to supporters.\n\
                      2. What this episode/video is about and exclusive creator insights.\n\
                      3. 1-2 open discussion questions to encourage patrons to leave comments below.";

        match llm.complete(prompt, &full_transcript) {
            Ok(patreon) => {
                let p = input_path.with_extension("patreon.txt");
                let _ = fs::write(&p, &patreon);
                let _ = tx.send(PipelineMessage::Log(format!("Saved Patreon post to: {}", p.display())));
                patreon_path_out = Some(p.display().to_string());
                patreon_text_out = Some(patreon);
            }
            Err(e) => {
                let _ = tx.send(PipelineMessage::Log(format!("Warning: Patreon post generation failed: {:#}", e)));
            }
        }
    }

    Ok(PipelineResult {
        transcript_path: transcript_path.display().to_string(),
        transcript_text: full_transcript,
        summary_path: summary_path_out,
        summary_text: summary_text_out,
        youtube_path: youtube_path_out,
        youtube_text: youtube_text_out,
        patreon_path: patreon_path_out,
        patreon_text: patreon_text_out,
    })
}