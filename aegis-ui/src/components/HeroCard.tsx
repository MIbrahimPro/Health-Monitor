import React from "react";
import { useAnimatedNumber } from "../hooks/useAnimatedNumber";
import "./HeroCard.css";

interface HeroCardProps {
  bpm10: number | null;
  bpm30: number | null;
  bpm60: number | null;
  faceFound: boolean;
  warmupProgress: number;
  isLoading?: boolean;
}

const getHrColorVar = (val: number | null) => {
  if (val === null) return "var(--text-low)";
  if (val < 60) return "var(--accent-2)";
  if (val > 100) return "var(--danger)";
  return "var(--accent)";
};

export const HeroCard: React.FC<HeroCardProps> = ({ bpm10, bpm30, bpm60, faceFound, warmupProgress, isLoading }) => {
  const display10 = useAnimatedNumber(bpm10);
  const display30 = useAnimatedNumber(bpm30);
  const display60 = useAnimatedNumber(bpm60);
  
  const hasLock = bpm10 !== null;
  const isCalibrating = !hasLock && faceFound && warmupProgress < 100;
  
  const colorVar = getHrColorVar(bpm10);
  
  // Heartbeat animation duration
  const beatDuration = bpm10 ? (60 / bpm10).toFixed(2) + "s" : "0s";

  return (
    <div className="card hero-card">
      <div className="hero-header">
        <h2 className="card-title">Heart Rate</h2>
        <svg 
          className={`hero-heart ${hasLock ? 'beating' : ''}`}
          style={{ 
            animationDuration: beatDuration,
            fill: hasLock ? colorVar : 'var(--text-low)'
          }}
          viewBox="0 0 24 24"
        >
          <path d="M12 21.35l-1.45-1.32C5.4 15.36 2 12.28 2 8.5 2 5.42 4.42 3 7.5 3c1.74 0 3.41.81 4.5 2.09C13.09 3.81 14.76 3 16.5 3 19.58 3 22 5.42 22 8.5c0 3.78-3.4 6.86-8.55 11.54L12 21.35z"/>
        </svg>
      </div>

      <div className="hero-value-container">
        {hasLock && <div className="hero-glow" style={{ background: `radial-gradient(closest-side, ${colorVar}20, transparent)` }} />}
        <div className={`hero-value ${isLoading ? 'skeleton' : !hasLock ? 'shimmer' : ''}`} style={{ color: isLoading ? 'transparent' : colorVar }}>
          {isLoading ? "00" : hasLock ? display10 : "--"}
        </div>
      </div>

      <div className="hero-footer">
        <div className="hero-chips">
          <div className="hero-chip">
            <span className="chip-label">30s</span>
            <span className="chip-value" style={{ color: getHrColorVar(bpm30) }}>
              {bpm30 !== null ? display30 : "--"}
            </span>
          </div>
          <div className="hero-chip">
            <span className="chip-label">60s</span>
            <span className="chip-value" style={{ color: getHrColorVar(bpm60) }}>
              {bpm60 !== null ? display60 : "--"}
            </span>
          </div>
        </div>
        
        {isCalibrating && (
          <div className="calibration-hint">
            <div className="calibration-track">
              <div className="calibration-fill" style={{ width: `${warmupProgress}%` }} />
            </div>
            <span>Calibrating...</span>
          </div>
        )}
      </div>
    </div>
  );
};
