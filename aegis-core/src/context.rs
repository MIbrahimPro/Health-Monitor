use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use rusqlite::{Connection, params};

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

        let _ = conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS samples (
                 ts INTEGER NOT NULL,
                 app TEXT NOT NULL,
                 title TEXT NOT NULL
             );"
        );

        while !stop.load(Ordering::Relaxed) {
            if let Some((app, title)) = active_window() {
                let ts = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64;
                let _ = conn.execute(
                    "INSERT INTO samples (ts, app, title) VALUES (?1, ?2, ?3)",
                    params![ts, app, title],
                );
            }
            thread::sleep(Duration::from_secs(5));
        }
    });
}

pub fn get_context_summary(hours: f64) -> Vec<(String, u64)> {
    let mut db_path = dirs::data_local_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    db_path.push("aegis");
    db_path.push("context.db");

    let conn = match Connection::open(&db_path) {
        Ok(c) => c,
        Err(_) => return vec![],
    };

    let ts_threshold = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64 - (hours * 3600.0) as i64;

    let mut stmt = match conn.prepare(
        "SELECT app, count(*) * 5 as seconds FROM samples WHERE ts >= ?1 GROUP BY app ORDER BY seconds DESC"
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    let rows = stmt.query_map(params![ts_threshold], |row| {
        let app: String = row.get(0)?;
        let seconds: i64 = row.get(1)?;
        Ok((app, seconds as u64))
    });

    match rows {
        Ok(iter) => iter.filter_map(Result::ok).collect(),
        Err(_) => vec![],
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
