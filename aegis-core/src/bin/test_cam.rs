use aegis_core::camera::start_camera_loop;
use tokio::sync::mpsc;
use std::time::Duration;

#[tokio::main]
async fn main() {
    let (tx, mut rx) = mpsc::channel(100);

    println!("Starting camera loop...");
    start_camera_loop(tx).expect("Failed to start");

    let timeout = tokio::time::sleep(Duration::from_secs(125));
    tokio::pin!(timeout);

    let mut frame_count = 0;
    let mut face_count = 0;
    let mut video_count = 0;
    let mut bpm_count = 0;
    let mut last_fps = 0.0_f32;

    loop {
        tokio::select! {
            Some(stats) = rx.recv() => {
                frame_count += 1;
                if stats.face_found { face_count += 1; }
                if stats.frame_base64.is_some() { video_count += 1; }
                if stats.bpm_10s.is_some() { bpm_count += 1; }
                if stats.fps > 0.0 { last_fps = stats.fps; }

                if frame_count % 50 == 0 || stats.bpm_10s.is_some() {
                    println!(
                        "[{:>4}] face={:<5} video={:<6} pulse={:>8.4} bpm(10/30/60)={}/{}/{} fps={:.1}",
                        frame_count,
                        stats.face_found,
                        stats.frame_base64.as_ref().map(|s| s.len()).unwrap_or(0),
                        stats.raw_pulse,
                        stats.bpm_10s.map(|b| format!("{:.1}", b)).unwrap_or_else(|| "--".to_string()),
                        stats.bpm_30s.map(|b| format!("{:.1}", b)).unwrap_or_else(|| "--".to_string()),
                        stats.bpm_60s.map(|b| format!("{:.1}", b)).unwrap_or_else(|| "--".to_string()),
                        stats.fps,
                    );
                }
            }
            _ = &mut timeout => {
                println!("\n--- TEST RESULTS (125 seconds) ---");
                println!("Total frames received:  {}", frame_count);
                println!("Effective FPS:          {:.1}", last_fps);
                println!("Frames with face:       {} ({:.0}%)", face_count, face_count as f32 / frame_count.max(1) as f32 * 100.0);
                println!("Frames with video:      {} ({:.0}%)", video_count, video_count as f32 / frame_count.max(1) as f32 * 100.0);
                println!("Frames with BPM:        {}", bpm_count);
                println!("---");
                if frame_count == 0 {
                    println!("FAIL: No frames received!");
                } else if last_fps < 5.0 {
                    println!("WARN: FPS very low ({:.1}), face detection may be blocking.", last_fps);
                } else if face_count == 0 {
                    println!("WARN: No face detected (is camera pointed at a face?)");
                } else {
                    println!("PASS: Pipeline working at {:.1} FPS!", last_fps);
                }
                break;
            }
        }
    }
}
