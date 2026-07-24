import React, { useState } from "react";

export const SecurityCard: React.FC<{
  ownerPresent: boolean | null;
  faceCount: number;
  jsdScore: number | null;
}> = ({ ownerPresent, faceCount, jsdScore }) => {
  const [enrolledFace, setEnrolledFace] = useState(true);
  const [enrolledTyping, setEnrolledTyping] = useState(true);

  return (
    <div className="card security-card">
      <h2 className="card-title">Security & Biometrics</h2>
      <div style={{ marginTop: '16px', display: 'flex', flexDirection: 'column', gap: '16px' }}>
        
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
          <span style={{ fontSize: '13px', color: 'var(--text-lo)' }}>Face Auth:</span>
          {enrolledFace ? (
            <span className={`status-chip ${ownerPresent ? 'running' : (ownerPresent === false ? 'danger' : 'idle')}`}>
              {ownerPresent === null ? 'Scanning...' : ownerPresent ? 'Owner Verified' : 'Unknown Face'}
            </span>
          ) : (
            <button className="action-btn" onClick={() => setEnrolledFace(true)}>Enroll Face</button>
          )}
        </div>

        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
          <span style={{ fontSize: '13px', color: 'var(--text-lo)' }}>Faces Detected:</span>
          <span style={{ color: faceCount >= 2 ? 'var(--danger)' : 'var(--text-hi)', fontWeight: faceCount >= 2 ? 'bold' : 'normal' }}>
            {faceCount} {faceCount >= 2 && '(Shoulder Surfing)'}
          </span>
        </div>

        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
          <span style={{ fontSize: '13px', color: 'var(--text-lo)' }}>Typing Rhythm (JSD):</span>
          {enrolledTyping ? (
            <span style={{ color: (jsdScore || 0) > 0.5 ? 'var(--danger)' : 'var(--primary)' }}>
              {jsdScore !== null ? jsdScore.toFixed(3) : '--'}
            </span>
          ) : (
            <button className="action-btn" onClick={() => setEnrolledTyping(true)}>Enroll Typing</button>
          )}
        </div>

      </div>
    </div>
  );
};
