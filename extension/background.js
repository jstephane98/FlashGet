import { buildCapture } from "./buildCapture.js";

const DEFAULT_PORT = 15151;
const KEEPALIVE_ALARM = "flashget-keepalive";
const PENDING_TTL_MS = 60000; // une capture en file de plus de 60 s est abandonnée
let socket = null;
let connected = false;
let reconnectTimer = null;
let connecting = false;
let pending = []; // captures en attente d'une connexion authentifiée : [{ url, ts }]

// Renvoie les captures mises en file une fois la connexion authentifiée (ignore les trop vieilles).
function flushPending() {
  if (!socket || socket.readyState !== WebSocket.OPEN || !connected) return;
  const now = Date.now();
  const items = pending;
  pending = [];
  for (const it of items) {
    if (now - it.ts > PENDING_TTL_MS) continue;
    try {
      socket.send(JSON.stringify(buildCapture(it.url)));
    } catch (_e) {
      // URL invalide : ignorée
    }
  }
}

async function getConfig() {
  const { token = "", port = DEFAULT_PORT } = await chrome.storage.local.get(["token", "port"]);
  return { token, port: Number(port) || DEFAULT_PORT };
}

function setConnected(value) {
  connected = value;
  // Pastille verte/grise sur l'icône.
  chrome.action.setBadgeText({ text: value ? "●" : "" });
  chrome.action.setBadgeBackgroundColor({ color: value ? "#22c55e" : "#9ca3af" });
  if (value) flushPending();
}

async function connect() {
  if (connecting) return;
  if (socket && (socket.readyState === WebSocket.OPEN || socket.readyState === WebSocket.CONNECTING)) {
    return;
  }
  connecting = true;
  let token, port;
  try {
    ({ token, port } = await getConfig());
  } catch (_e) {
    connecting = false;
    scheduleReconnect();
    return;
  }
  // Re-vérifier après l'await : un autre appel a pu ouvrir la socket entre-temps.
  if (socket && (socket.readyState === WebSocket.OPEN || socket.readyState === WebSocket.CONNECTING)) {
    connecting = false;
    return;
  }
  try {
    socket = new WebSocket(`ws://127.0.0.1:${port}`);
  } catch (_e) {
    connecting = false;
    scheduleReconnect();
    return;
  }
  connecting = false;
  socket.onopen = () => {
    socket.send(JSON.stringify({ type: "auth", token }));
  };
  socket.onmessage = (event) => {
    let msg;
    try {
      msg = JSON.parse(event.data);
    } catch (_e) {
      return;
    }
    if (msg.type === "authed") {
      setConnected(true);
    } else if (msg.type === "error" && (msg.error === "auth_failed" || msg.error === "need_auth")) {
      setConnected(false);
    }
  };
  socket.onclose = () => {
    setConnected(false);
    scheduleReconnect();
  };
  socket.onerror = () => {
    setConnected(false);
  };
}

function scheduleReconnect() {
  if (reconnectTimer) return;
  reconnectTimer = setTimeout(() => {
    reconnectTimer = null;
    connect();
  }, 3000);
}

// Envoie une capture. Si la connexion n'est pas prête (service worker réveillé à froid,
// reconnexion en cours), met l'URL en file et (re)connecte : elle partira après l'auth.
// Renvoie true si la capture est partie OU a été acceptée en file.
async function sendCapture(url) {
  if (socket && socket.readyState === WebSocket.OPEN && connected) {
    try {
      socket.send(JSON.stringify(buildCapture(url)));
      return true;
    } catch (_e) {
      return false; // URL invalide
    }
  }
  pending.push({ url, ts: Date.now() });
  if (pending.length > 20) pending.shift(); // borne anti-accumulation
  connect();
  return true; // acceptée : sera envoyée dès la (re)connexion
}

// Keepalive : réveille le service worker périodiquement (le min MV3 est 30 s) pour
// maintenir/relancer la connexion, sinon le SW est tué après ~30 s d'inactivité.
function ensureKeepalive() {
  chrome.alarms.create(KEEPALIVE_ALARM, { periodInMinutes: 0.5 });
}

chrome.alarms.onAlarm.addListener((alarm) => {
  if (alarm.name === KEEPALIVE_ALARM) connect();
});

// Menu contextuel sur liens/images/vidéos/audio.
chrome.runtime.onInstalled.addListener(() => {
  chrome.contextMenus.create({
    id: "flashget-capture",
    title: "Télécharger avec FlashGet",
    contexts: ["link", "image", "video", "audio"],
  });
  ensureKeepalive();
});

// Au démarrage du navigateur : reprogrammer le keepalive et se reconnecter.
chrome.runtime.onStartup.addListener(() => {
  ensureKeepalive();
  connect();
});

chrome.contextMenus.onClicked.addListener((info) => {
  const url = info.linkUrl || info.srcUrl || info.pageUrl;
  if (url) sendCapture(url);
});

// Messages depuis le popup.
chrome.runtime.onMessage.addListener((req, _sender, sendResponse) => {
  if (req.cmd === "status") {
    sendResponse({ connected });
    return false;
  }
  if (req.cmd === "send" && req.url) {
    sendCapture(req.url).then((ok) => sendResponse({ ok }));
    return true; // réponse asynchrone
  }
  if (req.cmd === "reconnect") {
    connect();
    sendResponse({ ok: true });
    return false;
  }
  return false;
});

// Connexion initiale + keepalive au (ré)veil du service worker.
ensureKeepalive();
connect();
