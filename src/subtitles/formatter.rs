// src/subtitles/formatter.rs
use anyhow::Result;
use crate::podman::client::WhisperSegment;
use crate::utils::format_srt_timestamp;

/// Minimum display time (500ms) to ensure subtitle readability and prevent 0s glitches
const MIN_SUBTITLE_DURATION_SECS: f64 = 0.5;

#[derive(Debug, Clone)]
pub struct Subtitle {
    pub index: usize,
    pub start: f64,
    pub end: f64,
    pub text: String,
}

/// Convert Whisper segments into formatted subtitles.
pub fn format(
    segments: &[WhisperSegment],
    max_chars_per_line: usize,
    two_lines: bool,
    min_gap_ms: u64,
) -> Result<Vec<Subtitle>> {
    let mut subtitles = Vec::new();

    // 1. Collect and wrap text for all segments
    for segment in segments {
        let text = segment.text.trim();
        if text.is_empty() {
            continue;
        }

        // Wrap all words into lines without dropping any text
        let lines = wrap_text(text, max_chars_per_line);
        if lines.is_empty() {
            continue;
        }

        // If two_lines is requested and we have > 2 lines, split into multiple subtitle cards
        let line_chunks: Vec<Vec<String>> = if two_lines && lines.len() > 2 {
            lines.chunks(2).map(|c| c.to_vec()).collect()
        } else {
            vec![lines]
        };

        let chunk_count = line_chunks.len();
        let total_duration = (segment.end - segment.start).max(MIN_SUBTITLE_DURATION_SECS);

        // Calculate total characters to distribute duration proportionally
        let total_chars: usize = line_chunks
            .iter()
            .map(|chunk| chunk.iter().map(|l| l.len()).sum::<usize>())
            .sum();

        let mut current_start = segment.start;

        for chunk in line_chunks {
            let chunk_chars: usize = chunk.iter().map(|l| l.len()).sum();
            let chunk_fraction = if total_chars > 0 && chunk_count > 1 {
                chunk_chars as f64 / total_chars as f64
            } else {
                1.0 / chunk_count as f64
            };

            let chunk_duration = (total_duration * chunk_fraction).max(MIN_SUBTITLE_DURATION_SECS);
            let chunk_end = current_start + chunk_duration;

            subtitles.push(Subtitle {
                index: subtitles.len() + 1,
                start: current_start,
                end: chunk_end,
                text: chunk.join("\n"),
            });

            current_start = chunk_end;
        }
    }

    // 2. Adjust gaps and eliminate zero-duration / inverted timestamps (Fix 2C)
    let gap_secs = (min_gap_ms as f64) / 1000.0;

    for i in 0..subtitles.len().saturating_sub(1) {
        // Ensure next subtitle cannot start before current subtitle has completed minimum duration
        let earliest_next_start = subtitles[i].start + MIN_SUBTITLE_DURATION_SECS + gap_secs;
        if subtitles[i + 1].start < earliest_next_start {
            subtitles[i + 1].start = earliest_next_start;
        }

        // Enforce the gap while guaranteeing minimum duration
        let target_end = subtitles[i + 1].start - gap_secs;
        subtitles[i].end = subtitles[i]
            .end
            .min(target_end)
            .max(subtitles[i].start + MIN_SUBTITLE_DURATION_SECS);
    }

    // Ensure the very last subtitle also meets the minimum duration
    if let Some(last) = subtitles.last_mut() {
        if last.end < last.start + MIN_SUBTITLE_DURATION_SECS {
            last.end = last.start + MIN_SUBTITLE_DURATION_SECS;
        }
    }

    // Re-index sequentially
    for (i, sub) in subtitles.iter_mut().enumerate() {
        sub.index = i + 1;
    }

    Ok(subtitles)
}

/// Split text into lines without breaking words or dropping text (Fix 2A)
fn wrap_text(text: &str, max_chars: usize) -> Vec<String> {
    if max_chars == 0 {
        return vec![text.to_string()];
    }

    let words: Vec<&str> = text.split_whitespace().collect();
    let mut lines = Vec::new();
    let mut current = String::new();

    for word in words {
        let proposed_len = if current.is_empty() {
            word.len()
        } else {
            current.len() + 1 + word.len()
        };

        if proposed_len <= max_chars {
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(word);
        } else {
            if !current.is_empty() {
                lines.push(current);
            }
            current = word.to_string();
        }
    }

    if !current.is_empty() {
        lines.push(current);
    }

    lines
}

/// Convert subtitles into SRT text.
pub fn to_srt(subtitles: &[Subtitle]) -> String {
    use std::fmt::Write;
    let mut output = String::with_capacity(subtitles.len() * 100);

    for subtitle in subtitles {
        let _ = writeln!(output, "{}", subtitle.index);
        let _ = writeln!(
            output,
            "{} --> {}",
            format_srt_timestamp(subtitle.start),
            format_srt_timestamp(subtitle.end)
        );
        output.push_str(&subtitle.text);
        output.push_str("\n\n");
    }

    output
}