use std::fs::{create_dir_all, File, OpenOptions};
use std::io::{BufRead, BufReader, stdout, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use chrono::Utc;
use serde_json::json;
use colored::*;
use crate::config::get_sb_dir;

pub fn get_recordings_dir() -> PathBuf {
    get_sb_dir().join("recordings")
}

pub struct SessionRecorder {
    pub file: File,
    #[allow(dead_code)]
    pub path: PathBuf,
    pub start_time: Instant,
}

impl SessionRecorder {
    pub fn new(session_id: &str, title: &str, width: u16, height: u16) -> Result<Self, String> {
        let dir = get_recordings_dir();
        let _ = create_dir_all(&dir);
        let path = dir.join(format!("{}.cast", session_id));

        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&path)
            .map_err(|e| format!("Konnte Aufzeichnungsdatei nicht erstellen: {}", e))?;

        // Asciinema v2 header
        let header = json!({
            "version": 2,
            "width": width,
            "height": height,
            "timestamp": Utc::now().timestamp(),
            "title": title,
            "env": {
                "TERM": "xterm-256color"
            }
        });

        writeln!(file, "{}", header.to_string())
            .map_err(|e| format!("Fehler beim Schreiben des Recording-Headers: {}", e))?;

        Ok(Self {
            file,
            path,
            start_time: Instant::now(),
        })
    }

    pub fn record_output(&mut self, data: &[u8]) {
        let elapsed = self.start_time.elapsed().as_secs_f64();
        let text = String::from_utf8_lossy(data);
        let event = json!([elapsed, "o", text]);
        let _ = writeln!(self.file, "{}", event.to_string());
        let _ = self.file.flush();
    }
}

pub async fn replay_session(target: &str) -> Result<(), String> {
    let path = if std::path::Path::new(target).exists() {
        PathBuf::from(target)
    } else {
        let dir = get_recordings_dir();
        let direct = dir.join(format!("{}.cast", target));
        if direct.exists() {
            direct
        } else {
            let mut found = None;
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if let Some(name) = p.file_stem().and_then(|s| s.to_str()) {
                        if name.contains(target) {
                            found = Some(p);
                            break;
                        }
                    }
                }
            }
            found.ok_or_else(|| format!("Aufzeichnung '{}' nicht gefunden in {}", target, dir.display()))?
        }
    };

    let file = File::open(&path)
        .map_err(|e| format!("Konnte Datei '{}' nicht öffnen: {}", path.display(), e))?;
    let mut reader = BufReader::new(file);

    println!("  {} Starte Replay von '{}'...\n", "►".bright_cyan(), path.display().to_string().bright_yellow());
    tokio::time::sleep(Duration::from_millis(600)).await;

    let mut first_line = String::new();
    if reader.read_line(&mut first_line).is_err() {
        return Err("Leere Aufzeichnungsdatei".to_string());
    }

    let mut last_timestamp = 0.0;
    let mut stdout = stdout();

    for line_res in reader.lines() {
        let line = match line_res {
            Ok(l) => l,
            Err(_) => continue,
        };

        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&line) {
            if let Some(arr) = val.as_array() {
                if arr.len() >= 3 {
                    let ts = arr[0].as_f64().unwrap_or(last_timestamp);
                    let event_type = arr[1].as_str().unwrap_or("");
                    let text = arr[2].as_str().unwrap_or("");

                    if event_type == "o" {
                        let delay = (ts - last_timestamp).max(0.0);
                        let sleep_duration = Duration::from_secs_f64(delay.min(1.0));
                        if sleep_duration.as_millis() > 5 {
                            tokio::time::sleep(sleep_duration).await;
                        }
                        last_timestamp = ts;

                        print!("{}", text);
                        let _ = stdout.flush();
                    }
                }
            }
        }
    }

    println!("\n\n  {} Replay beendet.", "✓".bright_green());
    Ok(())
}
