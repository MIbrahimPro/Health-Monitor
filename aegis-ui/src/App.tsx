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

      if (p.frame_base64) {
        setFrameBase64(p.frame_base64);
      }

      // Track warmup progress
      frameCountRef.current += 1;
      if (p.face_found) {
        setWarmupProgress(Math.min(100, (frameCountRef.current / 45) * 100));
      }

      // Smooth BPM displays
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

      // Pulse history for oscilloscope
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

    // Grid
    ctx.strokeStyle = "rgba(0, 255, 0, 0.08)";
    ctx.lineWidth = 1;
    ctx.beginPath();
    for (let i = 0; i < canvas.height; i += 30) {
      ctx.moveTo(0, i);
      ctx.lineTo(canvas.width, i);
    }
    for (let i = 0; i < canvas.width; i += 30) {
      ctx.moveTo(i, 0);
      ctx.lineTo(i, canvas.height);
    }
    ctx.stroke();

    // Center line
    ctx.strokeStyle = "rgba(0, 255, 0, 0.15)";
    ctx.beginPath();
    ctx.moveTo(0, canvas.height / 2);
    ctx.lineTo(canvas.width, canvas.height / 2);
    ctx.stroke();

    // ECG line
    ctx.beginPath();
    ctx.strokeStyle = "#00ff00";
    ctx.lineWidth = 2;
    ctx.lineJoin = "round";
    ctx.lineCap = "round";

    const maxVal = Math.max(...data, 0.001);
    const minVal = Math.min(...data, -0.001);
    const range = Math.max(maxVal - minVal, 0.001);

    const stepX = canvas.width / 300;

    for (let i = 0; i < data.length; i++) {
      const x = i * stepX;
      const normalized = (data[i] - minVal) / range;
      const y = canvas.height - (normalized * canvas.height * 0.8 + canvas.height * 0.1);

      if (i === 0) {
        ctx.moveTo(x, y);
      } else {
        ctx.lineTo(x, y);
      }
    }

    ctx.shadowBlur = 8;
    ctx.shadowColor = "#00ff00";
    ctx.stroke();
    ctx.shadowBlur = 0;
  };

  async function startTracking() {
    setStatus("Starting...");
    frameCountRef.current = 0;
    smoothedBpm10Ref.current = null;
    smoothedBpm30Ref.current = null;
    smoothedBpm60Ref.current = null;
    setWarmupProgress(0);
    try {
      const response = await invoke<string>("start_tracking");
      setStatus(response);
    } catch (e) {
      setStatus(`Error: ${e}`);
    }
  }

  const isTracking = status.includes("Tracking");
  
  const getBpmColor = (val: number | null) => {
    return val !== null ? (val < 60 ? "#00aaff" : val > 100 ? "#ff4444" : "#00ff00") : "#666";
  };

  return (
    <main className="aegis-app">
      {/* Header */}
      <div className="header">
        <div className="header-left">
          <h1 className="title">Aegis Vital Monitor</h1>
          <div className="status-row">
            <span className={`status-dot ${isTracking ? "active" : ""}`} />
            <span className={`status-text ${isTracking ? "active" : ""}`}>{status}</span>
            {isTracking && fps > 0 && (
              <span className="fps-badge">{fps.toFixed(0)} FPS</span>
            )}
          </div>
        </div>
        <button className="init-btn" onClick={startTracking}>
          Initialize Sensors
        </button>
      </div>

      {/* Main Content */}
      <div className="content">
        {/* Left: BPM + Waveform */}
        <div className="panel left-panel">
          {/* BPM Displays */}
          <div className="bpm-container" style={{ display: 'flex', gap: '2rem', flexWrap: 'wrap' }}>
            <div className="bpm-section">
              <div className="bpm-value" style={{ color: getBpmColor(bpm10) }}>
                {bpm10 !== null ? bpm10 : "--"}
              </div>
              <div className="bpm-label">BPM (10s avg)</div>
            </div>
            
            <div className="bpm-section">
              <div className="bpm-value" style={{ color: getBpmColor(bpm30), fontSize: '2.5rem' }}>
                {bpm30 !== null ? bpm30 : "--"}
              </div>
              <div className="bpm-label" style={{ fontSize: '0.9rem' }}>BPM (30s avg)</div>
            </div>
            
            <div className="bpm-section">
              <div className="bpm-value" style={{ color: getBpmColor(bpm60), fontSize: '2.5rem' }}>
                {bpm60 !== null ? bpm60 : "--"}
              </div>
              <div className="bpm-label" style={{ fontSize: '0.9rem' }}>BPM (60s avg)</div>
            </div>
          </div>
          
          {bpm10 === null && faceFound && warmupProgress < 100 && (
            <div className="warmup-bar">
              <div className="warmup-fill" style={{ width: `${warmupProgress}%` }} />
              <span className="warmup-text">Calibrating...</span>
            </div>
          )}

          {/* Oscilloscope */}
          <div className="oscilloscope-section" style={{ marginTop: '1rem' }}>
            <span className="section-label">rPPG Waveform</span>
            <div className="oscilloscope-container">
              <canvas
                ref={canvasRef}
                width={900}
                height={200}
                className="oscilloscope-canvas"
              />
            </div>
          </div>

          <p className="status-hint">
            {!isTracking
              ? 'Click "Initialize Sensors" to begin.'
              : faceFound
              ? bpm10 !== null
                ? "Pulse locked. Monitoring vitals."
                : "Face detected. Calibrating pulse signal..."
              : "No face detected. Please look at the camera."}
          </p>
        </div>

        {/* Right: Camera Feed */}
        <div className="panel right-panel">
          <span className="section-label">Camera Feed</span>
          <div className="camera-container">
            {frameBase64 ? (
              <img
                src={`data:image/jpeg;base64,${frameBase64}`}
                alt="Live Camera Feed"
                className="camera-feed"
              />
            ) : (
              <div className="no-signal">
                <span>No Signal</span>
              </div>
            )}
          </div>
        </div>
      </div>
    </main>
  );
}

export default App;
