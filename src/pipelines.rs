// src/pipelines.rs
pub mod content;
pub mod subtitles;
pub mod transcription;

use anyhow::Result;

#[derive(Clone, Debug)]
pub enum PipelineMessage<T> {
    Log(String),
    Progress { progress: f32, text: String },
    Finished(Result<T, String>),
}
