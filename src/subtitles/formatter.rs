// src/subtitles/formatter.rs
use crate::podman::client::WhisperSegment;
use crate::utils::format_srt_timestamp;
use anyhow::Result;

/// Minimum display time (500ms) to ensure subtitle readability and prevent 0s glitches
const MIN_SUBTITLE_DURATION_SECS: f64 = 0.5;

/// Maximum gap in seconds between subtitles across which orphan words will be merged
const MAX_MERGE_GAP_SECS: f64 = 3.5;

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
    let max_lines = if two_lines { 2 } else { 1 };

    // 1. Initial collection and word-wrapping for all segments
    for segment in segments {
        let text = segment.text.trim();
        if text.is_empty() {
            continue;
        }

        let lines = wrap_text(text, max_chars_per_line);
        if lines.is_empty() {
            continue;
        }

        // Group into subtitle cards of at most `max_lines` lines
        let line_chunks: Vec<Vec<String>> = if two_lines && lines.len() > 2 {
            lines.chunks(2).map(|c| c.to_vec()).collect()
        } else {
            vec![lines]
        };

        let chunk_count = line_chunks.len();
        let total_duration = (segment.end - segment.start).max(MIN_SUBTITLE_DURATION_SECS);

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

            // Balance internal lines if 2 lines are present
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

    // 2. Eliminate 1-2 word lines by shifting to previous or next subtitle
    eliminate_short_lines(&mut subtitles, max_chars_per_line, max_lines);

    // 3. Adjust gaps and enforce minimum duration constraints
    let gap_secs = (min_gap_ms as f64) / 1000.0;

    for i in 0..subtitles.len().saturating_sub(1) {
        let earliest_next_start = subtitles[i].start + MIN_SUBTITLE_DURATION_SECS + gap_secs;
        if subtitles[i + 1].start < earliest_next_start {
            subtitles[i + 1].start = earliest_next_start;
        }

        let target_end = subtitles[i + 1].start - gap_secs;
        subtitles[i].end = subtitles[i]
            .end
            .min(target_end)
            .max(subtitles[i].start + MIN_SUBTITLE_DURATION_SECS);
    }

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

fn word_count(s: &str) -> usize {
    s.split_whitespace().count()
}

/// Balance 2 lines so neither line is left with only 1 or 2 words if text can be shared.
fn balance_two_lines(line1: &str, line2: &str, max_chars: usize) -> Vec<String> {
    let combined = format!("{} {}", line1.trim(), line2.trim());
    if combined.len() <= max_chars {
        return vec![combined];
    }

    let mut words1: Vec<&str> = line1.split_whitespace().collect();
    let mut words2: Vec<&str> = line2.split_whitespace().collect();

    // If line 2 has <= 2 words, shift words from line 1 to line 2
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

    // If line 1 has <= 2 words, shift words from line 2 to line 1
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

/// Eliminates 1-2 word lines across subtitle boundaries.
fn eliminate_short_lines(subtitles: &mut Vec<Subtitle>, max_chars: usize, max_lines: usize) {
    let mut changed = true;
    let mut passes = 0;

    while changed && passes < 4 {
        changed = false;
        passes += 1;
        let mut i = 0;

        while i < subtitles.len() {
            let lines: Vec<String> = subtitles[i].text.lines().map(|s| s.to_string()).collect();

            // Scenario A: Entire subtitle is only 1 line with <= 2 words
            if lines.len() == 1 && word_count(&lines[0]) <= 2 {
                let text = lines[0].clone();

                // 1. Try moving into previous subtitle
                let can_merge_prev = i > 0
                    && (subtitles[i].start - subtitles[i - 1].end).abs() <= MAX_MERGE_GAP_SECS;

                if can_merge_prev {
                    let prev_lines: Vec<String> = subtitles[i - 1]
                        .text
                        .lines()
                        .map(|s| s.to_string())
                        .collect();

                    if prev_lines.len() == 1 {
                        // Previous subtitle has 1 line
                        if prev_lines[0].len() + 1 + text.len() <= max_chars {
                            // Fits on the same line
                            subtitles[i - 1].text = format!("{} {}", prev_lines[0], text);
                            subtitles[i - 1].end = subtitles[i].end;
                            subtitles.remove(i);
                            changed = true;
                            continue;
                        } else if max_lines >= 2 {
                            // Too long: add a break / new line and balance lines
                            let balanced = balance_two_lines(&prev_lines[0], &text, max_chars);
                            subtitles[i - 1].text = balanced.join("\n");
                            subtitles[i - 1].end = subtitles[i].end;
                            subtitles.remove(i);
                            changed = true;
                            continue;
                        }
                    } else if prev_lines.len() == 2 && max_lines >= 2 {
                        // Previous subtitle already has 2 lines: cannot add a 3rd line.
                        // Check if line 2 can absorb it without exceeding line limit.
                        if prev_lines[1].len() + 1 + text.len() <= max_chars {
                            subtitles[i - 1].text =
                                format!("{}\n{} {}", prev_lines[0], prev_lines[1], text);
                            subtitles[i - 1].end = subtitles[i].end;
                            subtitles.remove(i);
                            changed = true;
                            continue;
                        }
                    }
                }

                // 2. Previous subtitle could not accept it: move to next subtitle
                let can_merge_next = i + 1 < subtitles.len()
                    && (subtitles[i + 1].start - subtitles[i].end).abs() <= MAX_MERGE_GAP_SECS;

                if can_merge_next {
                    let next_lines: Vec<String> = subtitles[i + 1]
                        .text
                        .lines()
                        .map(|s| s.to_string())
                        .collect();

                    if next_lines.len() == 1 {
                        if text.len() + 1 + next_lines[0].len() <= max_chars {
                            subtitles[i + 1].text = format!("{} {}", text, next_lines[0]);
                            subtitles[i + 1].start = subtitles[i].start;
                            subtitles.remove(i);
                            changed = true;
                            continue;
                        } else if max_lines >= 2 {
                            let balanced = balance_two_lines(&text, &next_lines[0], max_chars);
                            subtitles[i + 1].text = balanced.join("\n");
                            subtitles[i + 1].start = subtitles[i].start;
                            subtitles.remove(i);
                            changed = true;
                            continue;
                        }
                    } else if next_lines.len() == 2 && max_lines >= 2 {
                        if text.len() + 1 + next_lines[0].len() <= max_chars {
                            subtitles[i + 1].text =
                                format!("{} {}\n{}", text, next_lines[0], next_lines[1]);
                            subtitles[i + 1].start = subtitles[i].start;
                            subtitles.remove(i);
                            changed = true;
                            continue;
                        } else {
                            // Re-wrap all words into 2 lines
                            let combined = format!("{} {}\n{}", text, next_lines[0], next_lines[1]);
                            let wrapped = wrap_text(&combined, max_chars);
                            if wrapped.len() <= max_lines {
                                let balanced = if wrapped.len() == 2 {
                                    balance_two_lines(&wrapped[0], &wrapped[1], max_chars)
                                } else {
                                    wrapped
                                };
                                subtitles[i + 1].text = balanced.join("\n");
                                subtitles[i + 1].start = subtitles[i].start;
                                subtitles.remove(i);
                                changed = true;
                                continue;
                            }
                        }
                    }
                }
            }

            // Scenario B: Subtitle has 2 lines, but Line 2 has <= 2 words
            if lines.len() == 2 && word_count(&lines[1]) <= 2 {
                let l2_text = lines[1].clone();
                let can_merge_next = i + 1 < subtitles.len()
                    && (subtitles[i + 1].start - subtitles[i].end).abs() <= MAX_MERGE_GAP_SECS;

                if can_merge_next {
                    let next_lines: Vec<String> = subtitles[i + 1]
                        .text
                        .lines()
                        .map(|s| s.to_string())
                        .collect();

                    let accepted = if next_lines.len() == 1 {
                        if l2_text.len() + 1 + next_lines[0].len() <= max_chars {
                            subtitles[i + 1].text = format!("{} {}", l2_text, next_lines[0]);
                            true
                        } else if max_lines >= 2 {
                            let balanced = balance_two_lines(&l2_text, &next_lines[0], max_chars);
                            subtitles[i + 1].text = balanced.join("\n");
                            true
                        } else {
                            false
                        }
                    } else if next_lines.len() == 2 && max_lines >= 2 {
                        if l2_text.len() + 1 + next_lines[0].len() <= max_chars {
                            subtitles[i + 1].text =
                                format!("{} {}\n{}", l2_text, next_lines[0], next_lines[1]);
                            true
                        } else {
                            let combined =
                                format!("{} {}\n{}", l2_text, next_lines[0], next_lines[1]);
                            let wrapped = wrap_text(&combined, max_chars);
                            if wrapped.len() <= max_lines {
                                let balanced = if wrapped.len() == 2 {
                                    balance_two_lines(&wrapped[0], &wrapped[1], max_chars)
                                } else {
                                    wrapped
                                };
                                subtitles[i + 1].text = balanced.join("\n");
                                true
                            } else {
                                false
                            }
                        }
                    } else {
                        false
                    };

                    if accepted {
                        // Split duration proportionally between the two cards
                        let cur_len = lines[0].len().max(1);
                        let moved_len = l2_text.len().max(1);
                        let total_len = (cur_len + moved_len) as f64;
                        let frac = (cur_len as f64) / total_len;

                        let duration =
                            (subtitles[i].end - subtitles[i].start).max(MIN_SUBTITLE_DURATION_SECS);
                        let split_time = subtitles[i].start + duration * frac;

                        subtitles[i].end =
                            split_time.max(subtitles[i].start + MIN_SUBTITLE_DURATION_SECS);
                        subtitles[i + 1].start = subtitles[i + 1].start.min(split_time);
                        subtitles[i].text = lines[0].clone();

                        changed = true;
                        i += 1;
                        continue;
                    }
                }
            }

            i += 1;
        }
    }
}

/// Split text into lines without breaking words or dropping text
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

/// Convert subtitles into standard SRT text.
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
