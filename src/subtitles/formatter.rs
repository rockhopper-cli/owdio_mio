use crate::clients::whisper::WhisperSegment;
use crate::utils::format_srt_timestamp;
use anyhow::Result;
use std::fmt::Write;

const MIN_SUBTITLE_DURATION_SECS: f64 = 0.5;
const MAX_MERGE_GAP_SECS: f64 = 0.6; // Only merge words across tight acoustic pauses

#[derive(Debug, Clone)]
pub struct Subtitle {
    pub index: usize,
    pub start: f64,
    pub end: f64,
    pub text: String,
}

pub fn format(
    segments: &[WhisperSegment],
    max_chars_per_line: usize,
    two_lines: bool,
    min_gap_ms: u64,
) -> Result<Vec<Subtitle>> {
    let mut subtitles = Vec::new();
    let chunk_size = if two_lines { 2 } else { 1 };

    for segment in segments {
        let text = segment.text.trim();
        if text.is_empty() {
            continue;
        }

        let lines = wrap_text(text, max_chars_per_line);
        if lines.is_empty() {
            continue;
        }

        // Split lines into chunks of 1 or 2 based on user settings
        let line_chunks: Vec<Vec<String>> = lines.chunks(chunk_size).map(|c| c.to_vec()).collect();

        let chunk_count = line_chunks.len();
        let total_duration = (segment.end - segment.start).max(MIN_SUBTITLE_DURATION_SECS);
        let total_chars: usize = line_chunks
            .iter()
            .map(|chunk| chunk.iter().map(|l| l.len()).sum::<usize>())
            .sum();

        let mut current_start = segment.start.max(0.0);

        for chunk in line_chunks {
            let chunk_chars: usize = chunk.iter().map(|l| l.len()).sum();
            let chunk_fraction = if total_chars > 0 && chunk_count > 1 {
                chunk_chars as f64 / total_chars as f64
            } else {
                1.0 / chunk_count as f64
            };

            let chunk_duration = (total_duration * chunk_fraction).max(MIN_SUBTITLE_DURATION_SECS);
            let chunk_end = current_start + chunk_duration;

            let formatted_text = if chunk.len() == 2 {
                balance_two_lines(&chunk[0], &chunk[1], max_chars_per_line).join("\n")
            } else {
                chunk.join("\n")
            };

            subtitles.push(Subtitle {
                index: subtitles.len() + 1,
                start: current_start,
                end: chunk_end,
                text: formatted_text,
            });

            current_start = chunk_end;
        }
    }

    eliminate_short_lines(&mut subtitles, max_chars_per_line, chunk_size);

    // Collision resolution and gap enforcement
    let gap_secs = (min_gap_ms as f64) / 1000.0;

    for i in 0..subtitles.len().saturating_sub(1) {
        // Guarantee minimal positive duration for subtitle i
        if subtitles[i].end < subtitles[i].start + MIN_SUBTITLE_DURATION_SECS {
            subtitles[i].end = subtitles[i].start + MIN_SUBTITLE_DURATION_SECS;
        }

        let earliest_next_start = subtitles[i].end + gap_secs;
        if subtitles[i + 1].start < earliest_next_start {
            // If the next subtitle overlaps, shift next start forward to avoid shrinking the current dialogue
            subtitles[i + 1].start = earliest_next_start;
            if subtitles[i + 1].end < subtitles[i + 1].start + MIN_SUBTITLE_DURATION_SECS {
                subtitles[i + 1].end = subtitles[i + 1].start + MIN_SUBTITLE_DURATION_SECS;
            }
        }
    }

    if let Some(last) = subtitles.last_mut()
        && last.end < last.start + MIN_SUBTITLE_DURATION_SECS
    {
        last.end = last.start + MIN_SUBTITLE_DURATION_SECS;
    }

    for (i, sub) in subtitles.iter_mut().enumerate() {
        sub.index = i + 1;
    }

    Ok(subtitles)
}

fn word_count(s: &str) -> usize {
    s.split_whitespace().count()
}

fn balance_two_lines(line1: &str, line2: &str, max_chars: usize) -> Vec<String> {
    let combined = format!("{} {}", line1.trim(), line2.trim());
    if combined.len() <= max_chars {
        return vec![combined];
    }

    let mut words1: Vec<&str> = line1.split_whitespace().collect();
    let mut words2: Vec<&str> = line2.split_whitespace().collect();

    while words2.len() <= 2 && words1.len() > 2 {
        let last_word = words1.last().unwrap();
        let candidate_len = if words2.is_empty() {
            last_word.len()
        } else {
            last_word.len() + 1 + words2.iter().map(|w| w.len()).sum::<usize>() + (words2.len() - 1)
        };

        if candidate_len <= max_chars {
            let moved = words1.pop().unwrap();
            words2.insert(0, moved);
        } else {
            break;
        }
    }

    while words1.len() <= 2 && words2.len() > 2 {
        let first_word = words2.first().unwrap();
        let candidate_len = if words1.is_empty() {
            first_word.len()
        } else {
            words1.iter().map(|w| w.len()).sum::<usize>()
                + (words1.len() - 1)
                + 1
                + first_word.len()
        };

        if candidate_len <= max_chars {
            let moved = words2.remove(0);
            words1.push(moved);
        } else {
            break;
        }
    }

    vec![words1.join(" "), words2.join(" ")]
}

fn eliminate_short_lines(subtitles: &mut Vec<Subtitle>, max_chars: usize, max_lines: usize) {
    let mut changed = true;
    let mut passes = 0;

    while changed && passes < 4 {
        changed = false;
        passes += 1;
        let mut i = 0;

        while i < subtitles.len() {
            let lines: Vec<String> = subtitles[i].text.lines().map(|s| s.to_string()).collect();

            // Case A: Subtitle contains a single line with only 1-2 words
            if lines.len() == 1 && word_count(&lines[0]) <= 2 {
                let text = lines[0].clone();

                // Merge backwards into previous subtitle if gap is negligible
                let can_merge_prev = i > 0
                    && (subtitles[i].start - subtitles[i - 1].end).abs() <= MAX_MERGE_GAP_SECS;

                if can_merge_prev {
                    let prev_lines: Vec<String> = subtitles[i - 1]
                        .text
                        .lines()
                        .map(|s| s.to_string())
                        .collect();

                    if prev_lines.len() == 1 {
                        if prev_lines[0].len() + 1 + text.len() <= max_chars {
                            subtitles[i - 1].text = format!("{} {}", prev_lines[0], text);
                            subtitles[i - 1].end = subtitles[i].end;
                            subtitles.remove(i);
                            changed = true;
                            continue;
                        } else if max_lines >= 2 {
                            let balanced = balance_two_lines(&prev_lines[0], &text, max_chars);
                            subtitles[i - 1].text = balanced.join("\n");
                            subtitles[i - 1].end = subtitles[i].end;
                            subtitles.remove(i);
                            changed = true;
                            continue;
                        }
                    } else if prev_lines.len() == 2
                        && max_lines >= 2
                        && prev_lines[1].len() + 1 + text.len() <= max_chars
                    {
                        subtitles[i - 1].text =
                            format!("{}\n{} {}", prev_lines[0], prev_lines[1], text);
                        subtitles[i - 1].end = subtitles[i].end;
                        subtitles.remove(i);
                        changed = true;
                        continue;
                    }
                }
            }

            // Case B: 2-line subtitle where line 2 is an orphan (1-2 words)
            if lines.len() == 2 && word_count(&lines[1]) <= 2 {
                let balanced = balance_two_lines(&lines[0], &lines[1], max_chars);
                if balanced.len() == 2 && word_count(&balanced[1]) > word_count(&lines[1]) {
                    subtitles[i].text = balanced.join("\n");
                    changed = true;
                }
            }

            i += 1;
        }
    }
}

fn wrap_text(text: &str, max_chars: usize) -> Vec<String> {
    if max_chars == 0 {
        return vec![text.to_string()];
    }

    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return Vec::new();
    }

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

pub fn to_srt(subtitles: &[Subtitle]) -> String {
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
