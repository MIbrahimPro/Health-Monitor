import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { HeroCard } from "./components/HeroCard";
import { RespCard } from "./components/RespCard";
import { QualityCard } from "./components/QualityCard";
import { WaveformCard } from "./components/WaveformCard";
import "./App.css";

function App() {
  const [bpm10, setBpm10] = useState<number | null>(null);
  const [bpm30, setBpm30] = useState<number | null>(null);
  const [bpm60, setBpm60] = useState<number | null>(null);
  const [faceFound, setFaceFound] = useState<boolean>(false);
  const [frameBase64, setFrameBase64] = useState<string | null>(null);
  const [status, setStatus] = useState<string>("Stopped");
  const [fps, setFps] = useState<number>(0);
  const [warmupProgress, setWarmupProgress] = useState<number>(0);
  const [respBpm, setRespBpm] = useState<number | null>(null);
  const [quality, setQuality] = useState<number>(0);
  const [snrDb, setSnrDb] = useState<number>(0);
  
  const pulseHistoryRef = useRef<number[]>([]);
  const smoothedBpm10Ref = useRef<number | null>(null);
  const smoothedBpm30Ref = useRef<number | null>(null);
  const smoothedBpm60Ref = useRef<number | null>(null);
  const frameCountRef = useRef<number>(0);

  useEffect(() => {
    const unlisten = listen<{
      pulse: number;
      bpm_10s: number | null;
      bpm_30s: number | null;
      bpm_60s: number | null;
      face_found: boolean;
      frame_base64: string | null;
      fps: number;
      resp_bpm: number | null;
      quality: number;
      snr_db: number;
    }>("pulse-update", (event) => {
      const p = event.payload;

      setFaceFound(p.face_found);
      if (p.fps > 0) setFps(p.fps);
      if (p.frame_base64) setFrameBase64(p.frame_base64);
      setRespBpm(p.resp_bpm);
      setQuality(p.quality);
      setSnrDb(p.snr_db);

      frameCountRef.current += 1;
      if (p.face_found) {
        setWarmupProgress(Math.min(100, (frameCountRef.current / 45) * 100));
      }

      const smoothBpm = (val: number | null, ref: React.MutableRefObject<number | null>, setter: React.Dispatch<React.SetStateAction<number | null>>) => {
        if (val !== null && val >= 40 && val <= 180) {
          if (ref.current === null) {
            ref.current = val;
          } else {
            ref.current = 0.85 * ref.current + 0.15 * val;
          }
          setter(Math.round(ref.current));
        }
      };

      smoothBpm(p.bpm_10s, smoothedBpm10Ref, setBpm10);
      smoothBpm(p.bpm_30s, smoothedBpm30Ref, setBpm30);
      smoothBpm(p.bpm_60s, smoothedBpm60Ref, setBpm60);

      pulseHistoryRef.current.push(p.pulse);
      if (pulseHistoryRef.current.length > 300) {
        pulseHistoryRef.current.shift();
      }
    });

    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  async function startTracking() {
    setStatus("Starting...");
    frameCountRef.current = 0;
    smoothedBpm10Ref.current = null;
    smoothedBpm30Ref.current = null;
    smoothedBpm60Ref.current = null;
    setBpm10(null);
    setBpm30(null);
    setBpm60(null);
    setWarmupProgress(0);
    try {
      const response = await invoke<string>("start_tracking");
      setStatus(response);
    } catch (e) {
      setStatus(`Error: ${e}`);
    }
  }

  async function stopTracking() {
    try {
      const response = await invoke<string>("stop_tracking");
      setStatus(response);
    } catch (e) {
      console.error(e);
    }
  }

  const isTracking = status.includes("Tracking");

  return (
    <div className="app-container">
      <div className="app-header">
        <div className="header-left">
          <span className="logo-mark">AEGIS</span>
          <span className={`status-chip ${isTracking ? 'running' : 'idle'}`}>
            {status}
          </span>
        </div>
        <div className="header-controls">
          {status.includes("Error") && (
            <div className="error-toast">{status}</div>
          )}
          <button 
            className={`action-btn ${!isTracking ? "primary" : "stop"}`} 
            onClick={isTracking ? stopTracking : startTracking}
            disabled={status === "Starting..."}
          >
            {status === "Starting..." ? "Starting..." : isTracking ? "Stop Monitoring" : "Start Monitoring"}
          </button>
        </div>
      </div>

      <div className="main-grid">
        <HeroCard 
          bpm10={bpm10} 
          bpm30={bpm30} 
          bpm60={bpm60} 
          faceFound={faceFound} 
          warmupProgress={warmupProgress} 
        />

        <RespCard respBpm={respBpm} />

        <QualityCard quality={quality} snrDb={snrDb} fps={fps} />

        <WaveformCard pulseHistoryRef={pulseHistoryRef} snrDb={snrDb} />

        <div className="card camera-card">
          <h2 className="card-title">Camera Feed</h2>
          
          <div className="camera-status-chip">
            {isTracking ? (
              faceFound ? (
                <span className="chip-live"><span className="dot"></span>LIVE</span>
              ) : (
                <span className="chip-noface"><span className="dot"></span>NO FACE</span>
              )
            ) : (
              <span className="chip-off"><span className="dot"></span>OFF</span>
            )}
          </div>

          {!faceFound && isTracking && (
            <div className="camera-overlay">
              <div className="overlay-pill">Position your face in view</div>
            </div>
          )}

          {frameBase64 ? (
            <img src={`data:image/jpeg;base64,${frameBase64}`} className="camera-feed" />
          ) : (
            <div className="camera-empty">
              No Signal
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

export default App;
