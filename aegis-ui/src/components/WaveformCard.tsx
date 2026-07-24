import React, { useEffect, useRef } from "react";
import "./WaveformCard.css";

interface WaveformCardProps {
  pulseHistoryRef: React.MutableRefObject<number[]>;
  snrDb: number;
}

export const WaveformCard: React.FC<WaveformCardProps> = ({ pulseHistoryRef, snrDb }) => {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const wrapperRef = useRef<HTMLDivElement>(null);
  const animRef = useRef<number>(0);

  useEffect(() => {
    const canvas = canvasRef.current;
    const wrapper = wrapperRef.current;
    if (!canvas || !wrapper) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    let width = 0;
    let height = 0;
    
    // Gradient and styling hoist
    let gradient: CanvasGradient | null = null;
    let gridPattern: CanvasPattern | null = null;

    const resize = () => {
      if (!wrapper) return;
      const dpr = window.devicePixelRatio || 1;
      const rect = wrapper.getBoundingClientRect();
      width = rect.width * dpr;
      height = rect.height * dpr;
      
      canvas.width = width;
      canvas.height = height;
      canvas.style.width = `${rect.width}px`;
      canvas.style.height = `${rect.height}px`;

      // Recreate gradient and pattern
      gradient = ctx.createLinearGradient(0, 0, width, 0);
      gradient.addColorStop(0, "var(--accent-2)");
      gradient.addColorStop(1, "var(--accent)");

      const patternCanvas = document.createElement("canvas");
      patternCanvas.width = 24 * dpr;
      patternCanvas.height = 24 * dpr;
      const pCtx = patternCanvas.getContext("2d");
      if (pCtx) {
        pCtx.fillStyle = "rgba(148, 163, 184, 0.06)";
        pCtx.beginPath();
        pCtx.arc(2 * dpr, 2 * dpr, 2 * dpr, 0, Math.PI * 2);
        pCtx.fill();
        gridPattern = ctx.createPattern(patternCanvas, "repeat");
      }
    };

    resize();
    window.addEventListener("resize", resize);

    const render = () => {
      animRef.current = requestAnimationFrame(render);
      if (!ctx || !gridPattern || !gradient) return;

      const data = pulseHistoryRef.current;
      
      // Clear
      ctx.clearRect(0, 0, width, height);

      // Draw grid
      ctx.fillStyle = gridPattern;
      ctx.fillRect(0, 0, width, height);

      if (data.length < 2) return;

      // Normalize amplitude with 10% padding
      const maxVal = Math.max(...data, 0.001);
      const minVal = Math.min(...data, -0.001);
      const range = Math.max(maxVal - minVal, 0.001);
      const padding = range * 0.1;
      const paddedRange = range + padding * 2;
      const paddedMin = minVal - padding;

      // Draw waveform right -> left
      ctx.beginPath();
      ctx.lineWidth = 2.5 * (window.devicePixelRatio || 1);
      ctx.lineJoin = "round";
      ctx.lineCap = "round";
      ctx.strokeStyle = gradient;

      const maxPoints = 300; // Match array size
      const stepX = width / Math.max(1, maxPoints - 1);
      
      // Draw from oldest to newest (newest at right)
      // Array has newest at the end
      let lastX = 0;
      let lastY = 0;

      for (let i = 0; i < data.length; i++) {
        // x is from right to left
        const x = width - ((data.length - 1 - i) * stepX);
        const normalized = (data[i] - paddedMin) / paddedRange;
        const y = height - (normalized * height);

        if (i === 0) {
          ctx.moveTo(x, y);
        } else {
          ctx.lineTo(x, y);
        }
        
        if (i === data.length - 1) {
          lastX = x;
          lastY = y;
        }
      }

      ctx.shadowBlur = 12;
      ctx.shadowColor = "rgba(45, 224, 165, 0.5)";
      ctx.stroke();
      ctx.shadowBlur = 0;

      // Comet head
      ctx.beginPath();
      ctx.fillStyle = "#ffffff";
      ctx.shadowBlur = 8;
      ctx.shadowColor = "rgba(255, 255, 255, 0.8)";
      ctx.arc(lastX, lastY, 4 * (window.devicePixelRatio || 1), 0, Math.PI * 2);
      ctx.fill();
      ctx.shadowBlur = 0;
    };

    animRef.current = requestAnimationFrame(render);

    return () => {
      cancelAnimationFrame(animRef.current);
      window.removeEventListener("resize", resize);
    };
  }, [pulseHistoryRef]);

  return (
    <div className="card waveform-card">
      <h2 className="card-title">rPPG Waveform</h2>
      <div className="waveform-wrapper" ref={wrapperRef}>
        <canvas ref={canvasRef} className="waveform-canvas" />
      </div>
      <div className="waveform-footer">
        <span>rPPG &middot; POS fused</span>
        <span>{snrDb.toFixed(1)} dB</span>
      </div>
    </div>
  );
};
