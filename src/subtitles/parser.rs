use std::fmt::Write;

/// Parses an SRT/VTT file into clean `[MM:SS] Text` lines for LLM context.
pub fn clean_srt_for_llm(content: &str) -> String {
    let mut cleaned = String::with_capacity(content.len());
    let mut current_timestamp = String::new();
    let mut current_text = Vec::new();

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            if !current_timestamp.is_empty() && !current_text.is_empty() {
                let _ = writeln!(
                    cleaned,
                    "[{}] {}",
                    current_timestamp,
                    current_text.join(" ")
                );
                current_timestamp.clear();
                current_text.clear();
            }
            continue;
        }

        if line.contains("-->") {
            if !current_timestamp.is_empty() && !current_text.is_empty() {
                let _ = writeln!(
                    cleaned,
                    "[{}] {}",
                    current_timestamp,
                    current_text.join(" ")
                );
                current_text.clear();
            }

            if let Some(start_time_token) = line.split("-->").next() {
                let clean_time = start_time_token.trim().replace('.', ",");
                let parts: Vec<&str> = clean_time.split(':').collect();

                if parts.len() == 3 {
                    // Standard SRT/VTT: HH:MM:SS,mmm
                    let h: u32 = parts[0].parse().unwrap_or(0);
                    let m = parts[1];
                    let s = parts[2].split(',').next().unwrap_or("00");

                    current_timestamp = if h > 0 {
                        format!("{:02}:{}:{}", h, m, s)
                    } else {
                        format!("{}:{}", m, s)
                    };
                } else if parts.len() == 2 {
                    // Standard WebVTT without hours: MM:SS,mmm
                    let m = parts[0];
                    let s = parts[1].split(',').next().unwrap_or("00");
                    current_timestamp = format!("{}:{}", m, s);
                }
            }
        } else if !current_timestamp.is_empty() {
            current_text.push(line);
        } else if !line.chars().all(|c| c.is_ascii_digit())
            && line != "WEBVTT"
            && !line.starts_with("NOTE")
        {
            let _ = writeln!(cleaned, "{}", line);
        }
    }

    if !current_timestamp.is_empty() && !current_text.is_empty() {
        let _ = writeln!(
            cleaned,
            "[{}] {}",
            current_timestamp,
            current_text.join(" ")
        );
    }

    if cleaned.is_empty() {
        content.to_string()
    } else {
        cleaned
    }
}
