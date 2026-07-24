use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use rusqlite::{Connection, params};

use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Intent {
    DeepWork,
    Research,
    Distraction,
    Idle,
}

impl Intent {
    pub fn as_str(&self) -> &'static str {
        match self {
            Intent::DeepWork => "DeepWork",
            Intent::Research => "Research",
            Intent::Distraction => "Distraction",
            Intent::Idle => "Idle",
        }
    }
    
    pub fn from_str(s: &str) -> Self {
        match s {
            "DeepWork" => Intent::DeepWork,
            "Research" => Intent::Research,
            "Distraction" => Intent::Distraction,
            _ => Intent::Idle,
        }
    }
}

pub fn classify(app: &str, title: &str) -> Intent {
    let app = app.to_lowercase();
    let title = title.to_lowercase();
    
    // Distraction
    if app.contains("youtube") || title.contains("youtube") {
        if !title.contains("tutorial") && !title.contains("course") && !title.contains("lecture") {
            return Intent::Distraction;
        }
    }
    if app.contains("reddit") || title.contains("reddit") {
        if !title.contains("r/rust") && !title.contains("r/programming") {
            return Intent::Distraction;
        }
    }
    let distractions = ["netflix", "twitch", "tiktok", "instagram", "facebook", "twitter", "x.com", "steam", "game"];
    for d in distractions {
        if app.contains(d) || title.contains(d) {
            return Intent::Distraction;
        }
    }
    
    // DeepWork
    let deep_work = ["code", "vim", "nvim", "emacs", "intellij", "pycharm", "terminal", "konsole", "alacritty", "kitty", "blender", "figma", "davinci"];
    for dw in deep_work {
        if app.contains(dw) || title.contains(dw) {
            return Intent::DeepWork;
        }
    }
    
    // Research
    let research = ["stackoverflow", "github", "docs.", "documentation", "arxiv", "scholar", "wikipedia", "chatgpt", "claude"];
    for r in research {
        if app.contains(r) || title.contains(r) {
            return Intent::Research;
        }
    }
    
    // Fallback
    let browsers = ["firefox", "chrome", "chromium", "brave", "safari", "edge", "opera", "browser"];
    for b in browsers {
        if app.contains(b) {
            return Intent::Research;
        }
    }
    
    Intent::DeepWork // Default fallback for unknown apps
}

pub fn active_window() -> Option<(String, String)> {
    let session = std::env::var("XDG_SESSION_TYPE").unwrap_or_default();
    if session == "wayland" {
        return None;
    }

    // Try xprop (X11)
    let output = std::process::Command::new("xprop")
        .args(&["-root", "_NET_ACTIVE_WINDOW"])
        .output()
        .ok()?;

    let out_str = String::from_utf8_lossy(&output.stdout);
    // _NET_ACTIVE_WINDOW(WINDOW): window id # 0x6e00003
    let id_str = out_str.split("window id # ").nth(1)?.trim();
    if id_str.is_empty() || id_str == "0x0" {
        return None;
    }

    let output2 = std::process::Command::new("xprop")
        .args(&["-id", id_str, "WM_CLASS", "_NET_WM_NAME"])
        .output()
        .ok()?;
    let props = String::from_utf8_lossy(&output2.stdout);

    // WM_CLASS(STRING) = "app", "App"
    // _NET_WM_NAME(UTF8_STRING) = "title"
    let mut app = String::new();
    let mut title = String::new();

    for line in props.lines() {
        if line.starts_with("WM_CLASS") {
            if let Some(parts) = line.split('=').nth(1) {
                // "app", "App"
                let c: Vec<&str> = parts.split(',').collect();
                if let Some(last) = c.last() {
                    app = last.trim().trim_matches('"').to_string();
                }
            }
        } else if line.starts_with("_NET_WM_NAME") {
            if let Some(parts) = line.split('=').nth(1) {
                title = parts.trim().trim_matches('"').to_string();
            }
        }
    }

    if app.is_empty() && title.is_empty() {
        None
    } else {
        Some((app, title))
    }
}

pub fn start_context_loop(stop: Arc<AtomicBool>) {
    let session = std::env::var("XDG_SESSION_TYPE").unwrap_or_default();
    if session == "wayland" {
        println!("context: wayland session — active-window unavailable");
    }

    thread::spawn(move || {
        let mut db_path = dirs::data_local_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
        db_path.push("aegis");
        std::fs::create_dir_all(&db_path).ok();
        db_path.push("context.db");

        let conn = match Connection::open(&db_path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("context: failed to open db: {}", e);
                return;
            }
        };

        let user_version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0)).unwrap_or(0);
        if user_version == 0 {
            let _ = conn.execute_batch(
                "PRAGMA journal_mode = WAL;
                 CREATE TABLE IF NOT EXISTS samples (
                     ts INTEGER NOT NULL,
                     app TEXT NOT NULL,
                     title TEXT NOT NULL
                 );
                 ALTER TABLE samples ADD COLUMN intent TEXT NOT NULL DEFAULT 'DeepWork';
                 PRAGMA user_version = 1;"
            );
        }

        while !stop.load(Ordering::Relaxed) {
            if let Some((app, title)) = active_window() {
                let intent = classify(&app, &title);
                let ts = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64;
                let _ = conn.execute(
                    "INSERT INTO samples (ts, app, title, intent) VALUES (?1, ?2, ?3, ?4)",
                    params![ts, app, title, intent.as_str()],
                );
            }
            thread::sleep(Duration::from_secs(5));
        }
    });
}

#[derive(Serialize)]
pub struct ContextSummary {
    pub top_apps: Vec<(String, u64)>,
    pub intent_split: std::collections::HashMap<String, u64>, // intent -> seconds
}

pub fn get_context_summary(hours: f64) -> ContextSummary {
    let mut db_path = dirs::data_local_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    db_path.push("aegis");
    db_path.push("context.db");

    let mut empty_summary = ContextSummary {
        top_apps: vec![],
        intent_split: std::collections::HashMap::new(),
    };

    let conn = match Connection::open(&db_path) {
        Ok(c) => c,
        Err(_) => return empty_summary,
    };

    let ts_threshold = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64 - (hours * 3600.0) as i64;

    // Top apps
    if let Ok(mut stmt) = conn.prepare("SELECT app, count(*) * 5 as seconds FROM samples WHERE ts >= ?1 GROUP BY app ORDER BY seconds DESC") {
        if let Ok(rows) = stmt.query_map(params![ts_threshold], |row| {
            let app: String = row.get(0)?;
            let seconds: i64 = row.get(1)?;
            Ok((app, seconds as u64))
        }) {
            empty_summary.top_apps = rows.filter_map(Result::ok).collect();
        }
    }
    
    // Intent split
    if let Ok(mut stmt) = conn.prepare("SELECT intent, count(*) * 5 as seconds FROM samples WHERE ts >= ?1 GROUP BY intent") {
        if let Ok(rows) = stmt.query_map(params![ts_threshold], |row| {
            let intent: String = row.get(0)?;
            let seconds: i64 = row.get(1)?;
            Ok((intent, seconds as u64))
        }) {
            for row in rows.filter_map(Result::ok) {
                empty_summary.intent_split.insert(row.0, row.1);
            }
        }
    }

    empty_summary
}

pub fn get_intent_now() -> Intent {
    if let Some((app, title)) = active_window() {
        classify(&app, &title)
    } else {
        Intent::Idle
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xprop_parser() {
        // Just mock the xprop parser logic
        let props = r#"WM_CLASS(STRING) = "navigator", "Firefox"
_NET_WM_NAME(UTF8_STRING) = "Mozilla Firefox""#;
        
        let mut app = String::new();
        let mut title = String::new();

        for line in props.lines() {
            if line.starts_with("WM_CLASS") {
                if let Some(parts) = line.split('=').nth(1) {
                    let c: Vec<&str> = parts.split(',').collect();
                    if let Some(last) = c.last() {
                        app = last.trim().trim_matches('"').to_string();
                    }
                }
            } else if line.starts_with("_NET_WM_NAME") {
                if let Some(parts) = line.split('=').nth(1) {
                    title = parts.trim().trim_matches('"').to_string();
                }
            }
        }
        assert_eq!(app, "Firefox");
        assert_eq!(title, "Mozilla Firefox");
    }
}
