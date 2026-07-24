import React from "react";
import { useAnimatedNumber } from "../hooks/useAnimatedNumber";
import "./RespCard.css";

interface RespCardProps {
  respBpm: number | null;
}

export const RespCard: React.FC<RespCardProps> = ({ respBpm }) => {
  const displayResp = useAnimatedNumber(respBpm);
  const hasLock = respBpm !== null;
  const animDuration = hasLock && respBpm > 0 ? (60 / respBpm).toFixed(2) + "s" : "0s";

  return (
    <div className="card resp-card">
      <h2 className="card-title">Respiration</h2>
      
      <div className="resp-content">
        <div className="card-value" style={{ color: hasLock ? 'var(--accent-warm)' : 'var(--text-low)' }}>
          {hasLock ? displayResp : "--"}
        </div>
        <div className="resp-unit">breaths/min</div>
        
        <div className="resp-wave-container">
          <svg 
            className={`resp-wave ${hasLock ? 'undulating' : ''}`} 
            style={{ animationDuration: animDuration }}
            viewBox="0 0 100 20" 
            preserveAspectRatio="none"
          >
            <path 
              d="M0,10 Q25,20 50,10 T100,10" 
              fill="none" 
              stroke={hasLock ? "var(--accent-warm)" : "var(--stroke)"} 
              strokeWidth="2" 
            />
            {/* Double the path to allow seamless CSS scrolling animation if desired, or just use a CSS transform animation */}
            <path 
              d="M100,10 Q125,20 150,10 T200,10" 
              fill="none" 
              stroke={hasLock ? "var(--accent-warm)" : "var(--stroke)"} 
              strokeWidth="2" 
            />
          </svg>
        </div>
      </div>
    </div>
  );
};
