import React, { useState } from "react";
import { invoke } from "@tauri-apps/api/core";

export const FocusCard: React.FC = () => {
  const [overlayEnabled, setOverlayEnabled] = useState(false);
  const [warmth, setWarmth] = useState(0.0);
  const [error, setError] = useState<string | null>(null);

  const toggleOverlay = async () => {
    try {
      if (overlayEnabled) {
        await invoke("overlay_disable");
        setOverlayEnabled(false);
      } else {
        await invoke("overlay_enable");
        setOverlayEnabled(true);
      }
      setError(null);
    } catch (e: any) {
      setError(e.toString());
    }
  };

  const handleWarmthChange = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const val = parseFloat(e.target.value);
    setWarmth(val);
    try {
      if (overlayEnabled) {
        await invoke("overlay_set_warmth", { warmth: val });
      }
    } catch (e) {
      console.error("Failed to set warmth", e);
    }
  };

  return (
    <div className="card focus-card">
      <h2 className="card-title">Focus & Context</h2>
      <div style={{ marginTop: '16px', display: 'flex', flexDirection: 'column', gap: '12px' }}>
        {error && <div className="error-toast">{error}</div>}
        
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
          <span style={{ fontSize: '13px', color: 'var(--text-hi)' }}>Screen Warmth Filter</span>
          <button 
            className={`action-btn ${overlayEnabled ? 'stop' : 'primary'}`}
            style={{ padding: '4px 12px', fontSize: '11px' }}
            onClick={toggleOverlay}
          >
            {overlayEnabled ? 'Disable' : 'Enable'}
          </button>
        </div>

        {overlayEnabled && (
          <div style={{ display: 'flex', alignItems: 'center', gap: '12px' }}>
            <span style={{ fontSize: '11px', color: 'var(--text-low)' }}>Intensity</span>
            <input 
              type="range" 
              min="0" 
              max="0.6" 
              step="0.05" 
              value={warmth} 
              onChange={handleWarmthChange} 
              style={{ flex: 1 }}
            />
          </div>
        )}
      </div>
    </div>
  );
};
