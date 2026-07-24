import React from "react";
import { useAnimatedNumber } from "../hooks/useAnimatedNumber";
import "./QualityCard.css";

interface QualityCardProps {
  quality: number; // 0-100
  snrDb: number;
  fps: number;
}

const getQualityColor = (val: number) => {
  if (val < 35) return "var(--danger)";
  if (val <= 65) return "var(--accent-warm)";
  return "var(--accent)";
};

export const QualityCard: React.FC<QualityCardProps> = ({ quality, snrDb, fps }) => {
  const displayQuality = useAnimatedNumber(quality);
  const colorVar = getQualityColor(quality);
  
  // SVG ring properties
  const radius = 32;
  const circumference = 2 * Math.PI * radius;
  // quality is 0-100, stroke-dashoffset = circumference - (quality / 100) * circumference
  const strokeDashoffset = circumference - (quality / 100) * circumference;

  return (
    <div className="card quality-card">
      <div className="quality-header">
        <h2 className="card-title">Signal Quality</h2>
      </div>
      
      <div className="quality-content">
        <div className="ring-container">
          <svg className="quality-ring" width="80" height="80" viewBox="0 0 80 80">
            {/* Background ring */}
            <circle 
              cx="40" 
              cy="40" 
              r={radius} 
              fill="none" 
              stroke="var(--stroke-strong)" 
              strokeWidth="6" 
            />
            {/* Progress ring */}
            <circle 
              cx="40" 
              cy="40" 
              r={radius} 
              fill="none" 
              stroke={colorVar} 
              strokeWidth="6"
              strokeDasharray={circumference}
              strokeDashoffset={strokeDashoffset}
              strokeLinecap="round"
              className="ring-progress"
            />
          </svg>
          <div className="ring-value" style={{ color: colorVar }}>
            {displayQuality}
          </div>
        </div>
        
        <div className="quality-stats">
          <span>SNR {snrDb.toFixed(1)} dB</span>
          <span>&middot;</span>
          <span>{fps.toFixed(1)} FPS</span>
        </div>
      </div>
    </div>
  );
};
