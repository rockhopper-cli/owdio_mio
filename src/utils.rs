// src/utils.rs

/// Formats seconds into a human-readable transcript timestamp:
/// - If duration >= 1 hour: "HH:MM:SS" (e.g., "01:23:45")
/// - If duration < 1 hour:  "MM:SS"    (e.g., "04:12")
pub fn format_transcript_timestamp(seconds: f64) -> String {
    let total_s = seconds.max(0.0).floor() as u64;
    let s = total_s % 60;
    let m = (total_s / 60) % 60;
    let h = total_s / 3600;

    if h > 0 {
        format!("{:02}:{:02}:{:02}", h, m, s)
    } else {
        format!("{:02}:{:02}", m, s)
    }
}

/// Formats seconds into standard SubRip (SRT) format:
/// "HH:MM:SS,mmm" (e.g., "00:01:23,456")
pub fn format_srt_timestamp(seconds: f64) -> String {
    let total_ms = (seconds.max(0.0) * 1000.0).round() as u64;

    let ms = total_ms % 1000;
    let total_s = total_ms / 1000;
    let s = total_s % 60;
    let m = (total_s / 60) % 60;
    let h = total_s / 3600;

    format!("{:02}:{:02}:{:02},{:03}", h, m, s, ms)
}

/// Returns current local time formatted as "HH:MM:SS" (e.g., "14:35:02")
pub fn current_log_timestamp() -> String {
    chrono::Local::now().format("%H:%M:%S").to_string()
}