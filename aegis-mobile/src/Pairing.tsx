import { useState } from 'react';

export function Pairing() {
  const [pairing, setPairing] = useState(false);

  const handlePair = async () => {
    setPairing(true);
    // Scaffold: would capture mic and listen for ultrasonic challenge
    // then POST to /api/pairing/answer
    setTimeout(() => {
      setPairing(false);
      alert('Pairing simulated');
    }, 2000);
  };

  return (
    <div className="card">
      <h2 className="card-title">Device Pairing</h2>
      <button className="action-btn" onClick={handlePair} disabled={pairing}>
        {pairing ? 'Listening for ultrasound...' : 'Verify Presence'}
      </button>
    </div>
  );
}
