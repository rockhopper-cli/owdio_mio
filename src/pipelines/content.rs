// src/pipelines/content.rs
use crate::clients::llm::LlmClient;
use crate::pipelines::PipelineMessage;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;

#[derive(Clone, Debug, Default)]
pub struct GeneratedContent {
    pub youtube: Option<(String, Option<String>)>, // (text, optional saved_path)
    pub patreon: Option<(String, Option<String>)>, // (text, optional saved_path)
}

pub fn run_content_generation(
    transcript: String,
    input_path: Option<PathBuf>,
    qwen_port: u16,
    gen_youtube: bool,
    gen_patreon: bool,
    cancel_token: Arc<AtomicBool>,
    tx: Sender<PipelineMessage<GeneratedContent>>,
) {
    let llm = LlmClient::new(qwen_port);
    let mut result = GeneratedContent::default();

    // 1. YouTube Description Generation
    if gen_youtube {
        if cancel_token.load(Ordering::Relaxed) {
            let _ = tx.send(PipelineMessage::Finished(Err("Cancelled by user".into())));
            return;
        }

        let _ = tx.send(PipelineMessage::Progress {
            progress: 0.5,
            text: "Generating YouTube chapters & description...".into(),
        });
        let _ = tx.send(PipelineMessage::Log(
            "[Content Generator] Querying Qwen for YouTube copy...".into(),
        ));

        let prompt = "You are an expert YouTube producer and SEO copywriter. \
                      Given the timestamped transcript from an SRT, produce an engaging YouTube video description.\n\
                      Structure requirements:\n\
                      1. Hook & Overview: 2-3 engaging sentences describing the video.\n\
                      2. Key Takeaways: 3-5 bullet points of what viewers will learn.\n\
                      3. TIMESTAMPS / CHAPTERS:\n\
                         - Identify natural topic transitions from the transcript timestamps.\n\
                         - The first chapter MUST start at '00:00 - Introduction' (or relevant title).\n\
                         - Format each timestamp strictly as 'MM:SS - Chapter Title' (or 'HH:MM:SS - Title').\n\
                      4. Call to Action: Remind viewers to like, subscribe, and comment.\n\
                      5. 5-8 relevant hashtags at the very bottom.";

        match llm.complete(prompt, &transcript) {
            Ok(yt) => {
                let path_str = input_path.as_ref().map(|p| {
                    let out_path = p.with_extension("youtube.txt");
                    let _ = fs::write(&out_path, &yt);
                    out_path.display().to_string()
                });
                result.youtube = Some((yt, path_str));
            }
            Err(e) => {
                let _ = tx.send(PipelineMessage::Finished(Err(format!(
                    "YouTube generation failed: {:#}",
                    e
                ))));
                return;
            }
        }
    }

    // 2. Patreon Community Post Generation
    if gen_patreon {
        if cancel_token.load(Ordering::Relaxed) {
            let _ = tx.send(PipelineMessage::Finished(Err("Cancelled by user".into())));
            return;
        }

        let _ = tx.send(PipelineMessage::Progress {
            progress: 0.9,
            text: "Generating Patreon community post...".into(),
        });
        let _ = tx.send(PipelineMessage::Log(
            "[Content Generator] Querying Qwen for Patreon post...".into(),
        ));

        let prompt = "You are a creator writing directly to your patrons/supporters.\n\
                      Given the transcript, write a warm, engaging Patreon post.\n\
                      Include:\n\
                      1. A warm personal greeting and thank you to supporters.\n\
                      2. What this episode/video is about, including exclusive creator insights.\n\
                      3. 1-2 open discussion questions to encourage patrons to leave comments below.";

        match llm.complete(prompt, &transcript) {
            Ok(patreon) => {
                let path_str = input_path.as_ref().map(|p| {
                    let out_path = p.with_extension("patreon.txt");
                    let _ = fs::write(&out_path, &patreon);
                    out_path.display().to_string()
                });
                result.patreon = Some((patreon, path_str));
            }
            Err(e) => {
                let _ = tx.send(PipelineMessage::Finished(Err(format!(
                    "Patreon generation failed: {:#}",
                    e
                ))));
                return;
            }
        }
    }

    let _ = tx.send(PipelineMessage::Finished(Ok(result)));
}
