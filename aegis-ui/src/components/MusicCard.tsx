import React, { useState } from "react";
import { invoke } from "@tauri-apps/api/core";

export const MusicCard: React.FC = () => {
  const [currentTag, setCurrentTag] = useState<string>("Focus");
  const [isPlaying, setIsPlaying] = useState(false);

  const handlePlayTag = async (tag: string) => {
    setCurrentTag(tag);
    setIsPlaying(true);
    try {
      const resp = await invoke<string>("music_play_tag", { tag });
      console.log(resp);
    } catch (e) {
      console.error(e);
    }
  };

  return (
    <div className="card music-card">
      <h2 className="card-title">Smart Player</h2>
      <div style={{ marginTop: '16px', display: 'flex', flexDirection: 'column', gap: '12px' }}>
        <div style={{ fontSize: '13px', color: 'var(--text-hi)' }}>
          Now Playing: <span style={{ color: 'var(--primary)', fontWeight: '500' }}>{isPlaying ? 'Scaffolded Track' : 'None'}</span>
        </div>
        
        <div style={{ display: 'flex', gap: '8px', flexWrap: 'wrap' }}>
          {['Calm', 'Focus', 'Energetic', 'Sad', 'Motivational'].map(tag => (
            <button 
              key={tag}
              className={`action-btn ${currentTag === tag ? 'primary' : ''}`}
              style={{ padding: '4px 8px', fontSize: '11px', flex: '1 0 30%' }}
              onClick={() => handlePlayTag(tag)}
            >
              {tag}
            </button>
          ))}
        </div>
        
        <div style={{ display: 'flex', justifyContent: 'center', gap: '16px', marginTop: '8px' }}>
          <button className="action-btn" onClick={() => setIsPlaying(false)}>Pause</button>
          <button className="action-btn primary" onClick={() => handlePlayTag(currentTag)}>Play/Skip</button>
        </div>
      </div>
    </div>
  );
};
