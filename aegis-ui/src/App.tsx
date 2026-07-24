import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
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
  
  // suppress TS unused errors for variables we will use fully in steps 2-6
  void faceFound;
  void fps;
  void warmupProgress;
  
  const pulseHistoryRef = useRef<number[]>([]);
  const canvasRef = useRef<HTMLCanvasElement>(null);
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
    }>("pulse-update", (event) => {
      const p = event.payload;

      setFaceFound(p.face_found);
      if (p.fps > 0) setFps(p.fps);
      if (p.frame_base64) setFrameBase64(p.frame_base64);

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
      drawOscilloscope();
    });

    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  const drawOscilloscope = () => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const data = pulseHistoryRef.current;
    if (data.length < 2) return;

    ctx.clearRect(0, 0, canvas.width, canvas.height);
    
    // Draw simple line for now, will be rewritten in Step 4
    ctx.beginPath();
    ctx.strokeStyle = "#2DE0A5";
    ctx.lineWidth = 2;
    
    const maxVal = Math.max(...data, 0.001);
    const minVal = Math.min(...data, -0.001);
    const range = Math.max(maxVal - minVal, 0.001);
    const stepX = canvas.width / Math.max(1, data.length - 1);

    for (let i = 0; i < data.length; i++) {
      const x = i * stepX;
      const normalized = (data[i] - minVal) / range;
      const y = canvas.height - (normalized * canvas.height * 0.8 + canvas.height * 0.1);
      if (i === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    }
    ctx.stroke();
  };

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
          <button 
            className={!isTracking ? "primary" : ""} 
            onClick={isTracking ? stopTracking : startTracking}
          >
            {isTracking ? "Stop" : "Start Monitoring"}
          </button>
        </div>
      </div>

      <div className="main-grid">
        <div className="card hero-card">
          <h2 className="card-title">Heart Rate</h2>
          <div className="card-value" style={{ color: bpm10 ? 'var(--accent)' : 'var(--text-low)' }}>
            {bpm10 !== null ? bpm10 : "--"}
          </div>
          <div className="card-subtext">
            30s · {bpm30 ?? "--"} &nbsp; 60s · {bpm60 ?? "--"}
          </div>
        </div>

        <div className="card resp-card">
          <h2 className="card-title">Respiration</h2>
          <div className="card-value" style={{ color: 'var(--accent-warm)' }}>--</div>
        </div>

        <div className="card quality-card">
          <h2 className="card-title">Signal Quality</h2>
          <div className="card-value">--</div>
        </div>

        <div className="card waveform-card">
          <h2 className="card-title">rPPG Waveform</h2>
          <div style={{ flex: 1, position: 'relative' }}>
            <canvas 
              ref={canvasRef} 
              style={{ position: 'absolute', top: 0, left: 0, width: '100%', height: '100%' }} 
            />
          </div>
        </div>

        <div className="card camera-card">
          <h2 className="card-title">Camera Feed</h2>
          {frameBase64 ? (
            <img src={`data:image/jpeg;base64,${frameBase64}`} className="camera-feed" />
          ) : (
            <div style={{ padding: '40px', color: 'var(--text-low)', textAlign: 'center', flex: 1, display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
              No Signal
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

export default App;
