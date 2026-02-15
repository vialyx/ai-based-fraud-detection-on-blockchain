// ── Configuration ────────────────────────────────────────────────────
const API_BASE = window.location.origin;
const WS_URL   = `${window.location.protocol === 'https:' ? 'wss' : 'ws'}://${window.location.host}/ws`;
const POLL_INTERVAL_MS = 5000;
const MAX_LIVE_ROWS    = 100;

// ── DOM references ───────────────────────────────────────────────────
const $statBlocks  = document.getElementById('stat-blocks');
const $statTxs     = document.getElementById('stat-txs');
const $statAlerts  = document.getElementById('stat-alerts');
const $liveBody    = document.getElementById('live-alerts-body');
const $scoresBody  = document.getElementById('scores-body');
const $alertsBody  = document.getElementById('alerts-body');
const $wsDot       = document.querySelector('#ws-status .dot');
const $wsLabel     = document.getElementById('ws-label');

// ── Helpers ──────────────────────────────────────────────────────────
function truncHash(hash) {
  if (!hash || hash.length < 14) return hash || '';
  return hash.slice(0, 8) + '…' + hash.slice(-6);
}

function riskBadge(level) {
  const l = (level || 'low').toLowerCase();
  return `<span class="badge badge-${l}">${l}</span>`;
}

function scoreCell(score) {
  const pct = Math.round(score * 100);
  const color = score >= 0.85 ? 'var(--red)'
              : score >= 0.65 ? 'var(--orange)'
              : score >= 0.40 ? 'var(--yellow)'
              : 'var(--green)';
  return `<div class="score-cell">
    <span>${score.toFixed(3)}</span>
    <div class="score-bar">
      <div class="score-bar-fill" style="width:${pct}%;background:${color}"></div>
    </div>
  </div>`;
}

function formatTime(iso) {
  if (!iso) return '';
  const d = new Date(iso);
  return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });
}

function formatNumber(n) {
  return Number(n || 0).toLocaleString();
}

// ── Stats polling ────────────────────────────────────────────────────
async function fetchStats() {
  try {
    const res = await fetch(`${API_BASE}/api/stats`);
    const data = await res.json();
    $statBlocks.textContent = formatNumber(data.blocks_processed);
    $statTxs.textContent    = formatNumber(data.transactions_scored);
    $statAlerts.textContent = formatNumber(data.alerts_raised);
  } catch (e) {
    console.warn('Failed to fetch stats:', e);
  }
}

// ── Scores polling ───────────────────────────────────────────────────
async function fetchScores() {
  try {
    const res  = await fetch(`${API_BASE}/api/scores`);
    const data = await res.json();
    if (!data.scores || data.scores.length === 0) {
      $scoresBody.innerHTML = '<tr class="empty-row"><td colspan="6">No scores yet.</td></tr>';
      return;
    }
    $scoresBody.innerHTML = data.scores
      .slice(-50)
      .reverse()
      .map(s => `<tr>
        <td class="tx-hash">${truncHash(s.tx_hash)}</td>
        <td>${s.block_number}</td>
        <td>${scoreCell(s.score)}</td>
        <td>${riskBadge(s.risk_level)}</td>
        <td>${(s.triggered_rules || []).join(', ') || '—'}</td>
        <td>${formatTime(s.scored_at)}</td>
      </tr>`).join('');
  } catch (e) {
    console.warn('Failed to fetch scores:', e);
  }
}

// ── Alerts polling ───────────────────────────────────────────────────
async function fetchAlerts() {
  try {
    const res  = await fetch(`${API_BASE}/api/alerts`);
    const data = await res.json();
    if (!data.alerts || data.alerts.length === 0) {
      $alertsBody.innerHTML = '<tr class="empty-row"><td colspan="7">No alerts yet.</td></tr>';
      return;
    }
    $alertsBody.innerHTML = data.alerts
      .slice(-50)
      .reverse()
      .map(a => `<tr>
        <td class="tx-hash">${truncHash(a.fraud_score?.tx_hash || '')}</td>
        <td class="tx-hash">${truncHash(a.from)}</td>
        <td class="tx-hash">${a.to ? truncHash(a.to) : '—'}</td>
        <td>${formatNumber(a.value)}</td>
        <td>${scoreCell(a.fraud_score?.score || 0)}</td>
        <td>${riskBadge(a.fraud_score?.risk_level)}</td>
        <td>${formatTime(a.created_at)}</td>
      </tr>`).join('');
  } catch (e) {
    console.warn('Failed to fetch alerts:', e);
  }
}

// ── WebSocket live feed ──────────────────────────────────────────────
let ws = null;
let liveRowCount = 0;

function setWsStatus(connected) {
  $wsDot.className = connected ? 'dot dot-connected' : 'dot dot-disconnected';
  $wsLabel.textContent = connected ? 'Live' : 'Disconnected';
}

function connectWs() {
  ws = new WebSocket(WS_URL);

  ws.onopen = () => {
    setWsStatus(true);
    console.log('WebSocket connected');
  };

  ws.onmessage = (event) => {
    try {
      const alert = JSON.parse(event.data);
      addLiveAlert(alert);
      // Also refresh stats immediately on new alert
      fetchStats();
    } catch (e) {
      console.warn('WS parse error:', e);
    }
  };

  ws.onclose = () => {
    setWsStatus(false);
    console.log('WebSocket disconnected – reconnecting in 3 s');
    setTimeout(connectWs, 3000);
  };

  ws.onerror = (err) => {
    console.warn('WebSocket error:', err);
    ws.close();
  };
}

function addLiveAlert(alert) {
  // Remove "Waiting" placeholder
  const empty = $liveBody.querySelector('.empty-row');
  if (empty) empty.remove();

  const row = document.createElement('tr');
  row.className = 'flash-row';
  row.innerHTML = `
    <td>${formatTime(alert.created_at || new Date().toISOString())}</td>
    <td class="tx-hash">${truncHash(alert.fraud_score?.tx_hash || '')}</td>
    <td>${scoreCell(alert.fraud_score?.score || 0)}</td>
    <td>${riskBadge(alert.fraud_score?.risk_level)}</td>
    <td>${(alert.fraud_score?.triggered_rules || []).join(', ') || '—'}</td>
  `;
  $liveBody.prepend(row);

  liveRowCount++;
  // Cap rows
  while (liveRowCount > MAX_LIVE_ROWS) {
    $liveBody.removeChild($liveBody.lastElementChild);
    liveRowCount--;
  }
}

// ── Bootstrap ────────────────────────────────────────────────────────
(async function init() {
  // Initial data fetch
  await Promise.all([fetchStats(), fetchScores(), fetchAlerts()]);

  // Start polling
  setInterval(fetchStats, POLL_INTERVAL_MS);
  setInterval(fetchScores, POLL_INTERVAL_MS);
  setInterval(fetchAlerts, POLL_INTERVAL_MS);

  // Connect WebSocket
  connectWs();
})();
