import React, { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

interface ContextSummary {
  top_apps: [string, number][];
  intent_split: Record<string, number>;
}

export const FocusCard: React.FC = () => {
  const [overlayEnabled, setOverlayEnabled] = useState(false);
  const [warmth, setWarmth] = useState(0.0);
  const [error, setError] = useState<string | null>(null);
  
  const [currentIntent, setCurrentIntent] = useState<string>("Idle");
  const [summary, setSummary] = useState<ContextSummary>({ top_apps: [], intent_split: {} });

  useEffect(() => {
    const fetchContext = async () => {
      try {
        const intent = await invoke<string>("get_intent_now");
        setCurrentIntent(intent);
        const sum = await invoke<ContextSummary>("get_context_summary", { hours: 2.0 });
        setSummary(sum);
      } catch (e) {
        console.error("Failed to fetch context", e);
      }
    };
    
    fetchContext();
    const timer = setInterval(fetchContext, 5000);
    return () => clearInterval(timer);
  }, []);

  const totalSeconds = Object.values(summary.intent_split).reduce((a, b) => a + b, 0) || 1;
  const dwPercent = ((summary.intent_split["DeepWork"] || 0) / totalSeconds) * 100;
  const resPercent = ((summary.intent_split["Research"] || 0) / totalSeconds) * 100;
  const distPercent = ((summary.intent_split["Distraction"] || 0) / totalSeconds) * 100;


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
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
        <h2 className="card-title">Focus & Context</h2>
        <span className={`status-chip ${currentIntent.toLowerCase()}`}>
          {currentIntent === "DeepWork" ? "Deep Work" : currentIntent}
        </span>
      </div>
      
      <div style={{ marginTop: '16px', display: 'flex', flexDirection: 'column', gap: '16px' }}>
        {/* Intent Stacked Bar */}
        <div>
          <div style={{ fontSize: '11px', color: 'var(--text-low)', marginBottom: '4px' }}>Session split (last 2h)</div>
          <div style={{ display: 'flex', height: '6px', borderRadius: '3px', overflow: 'hidden', background: 'var(--bg-card)' }}>
            <div style={{ width: `${dwPercent}%`, background: 'var(--primary)', transition: 'width 0.5s' }}></div>
            <div style={{ width: `${resPercent}%`, background: 'var(--warning)', transition: 'width 0.5s' }}></div>
            <div style={{ width: `${distPercent}%`, background: '#f87171', transition: 'width 0.5s' }}></div>
          </div>
          <div style={{ display: 'flex', gap: '12px', marginTop: '6px', fontSize: '10px', color: 'var(--text-low)' }}>
            <span style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
              <span style={{ width: '6px', height: '6px', borderRadius: '50%', background: 'var(--primary)' }}></span> Deep Work
            </span>
            <span style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
              <span style={{ width: '6px', height: '6px', borderRadius: '50%', background: 'var(--warning)' }}></span> Research
            </span>
            <span style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
              <span style={{ width: '6px', height: '6px', borderRadius: '50%', background: '#f87171' }}></span> Distraction
            </span>
          </div>
        </div>

        {/* Top Apps List */}
        <div>
          <div style={{ fontSize: '11px', color: 'var(--text-low)', marginBottom: '8px' }}>Top Applications</div>
          {summary.top_apps.slice(0, 3).map(([app, secs]) => (
            <div key={app} style={{ display: 'flex', justifyContent: 'space-between', fontSize: '12px', marginBottom: '4px' }}>
              <span style={{ color: 'var(--text-hi)', textTransform: 'capitalize' }}>{app || 'Unknown'}</span>
              <span style={{ color: 'var(--text-low)', fontVariantNumeric: 'tabular-nums' }}>{Math.round(secs / 60)}m</span>
            </div>
          ))}
          {summary.top_apps.length === 0 && (
            <div style={{ fontSize: '12px', color: 'var(--text-low)' }}>Waiting for activity...</div>
          )}
        </div>

        <hr style={{ borderTop: '1px solid rgba(255,255,255,0.05)', borderBottom: 'none' }}/>

        {error && <div className="error-toast">{error}</div>}
        
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
          <span style={{ fontSize: '12px', color: 'var(--text-hi)' }}>Comfort Overlay</span>
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
              style={{ flex: 1, accentColor: 'var(--primary)' }}
            />
          </div>
        )}
      </div>
    </div>
  );
};
