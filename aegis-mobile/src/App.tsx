import { useEffect, useState } from 'react';
import './App.css';
import { Pairing } from './Pairing';

function App() {
  const [vitals, setVitals] = useState<any>(null);
  
  useEffect(() => {
    // Scaffold WebSocket connection to daemon
  }, []);

  return (
    <div className="layout">
      <div className="top-nav">
        <h1>Aegis Mobile</h1>
      </div>
      <div className="dashboard-grid">
        <div className="card hero-card">
          <h2 className="card-title">Heart Rate</h2>
          <div className="value-display">
            <span className="value">{vitals?.bpm || '--'}</span>
            <span className="unit">bpm</span>
          </div>
        </div>
        <div className="card">
          <h2 className="card-title">Security</h2>
          <div>
            Owner Present: {vitals?.owner_present ? 'Yes' : 'No'}
          </div>
        </div>
        <Pairing />
      </div>
    </div>
  );
}

export default App;
