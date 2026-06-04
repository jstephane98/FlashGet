import { buildCapture } from "./buildCapture.js";
import { shouldCapture, parseCaptureTypes, DEFAULT_CAPTURE_TYPES } from "./capture-rules.js";

const DEFAULT_PORT = 15151;
const KEEPALIVE_ALARM = "flashget-keepalive";
const PENDING_TTL_MS = 60000; // une capture en file de plus de 60 s est abandonnée
let socket = null;
let connected = false;
let reconnectTimer = null;
let connecting = false;
let pending = []; // captures en attente d'une connexion authentifiée : [{ url, referrer, filename, ts }]

// Renvoie les captures mises en file une fois la connexion authentifiée (ignore les trop vieilles).
async function flushPending() {
  if (!socket || socket.readyState !== WebSocket.OPEN || !connected) return;
  const now = Date.now();
  const items = pending;
  pending = [];
  for (const it of items) {
    if (now - it.ts > PENDING_TTL_MS) continue;
    try {
      const headers = await gatherHeaders(it.url, it.referrer);
      socket.send(JSON.stringify(buildCapture(it.url, { ...headers, filename: it.filename })));
    } catch (_e) {
      // URL invalide ou erreur de collecte : on ignore cette capture
    }
  }
}

// Collecte les en-têtes navigateur pour une URL capturée : cookies (incl. httpOnly via
// l'API chrome.cookies), referer (page d'origine), user-agent.
async function gatherHeaders(url, referrer) {
  let cookie = "";
  try {
    const cookies = await chrome.cookies.getAll({ url });
    // Retirer les caractères de contrôle (ex. \r\n) des valeurs pour éviter toute
    // corruption / injection d'en-tête Cookie ; les cookies normaux ne sont pas affectés.
    const clean = (s) => String(s).replace(/[\x00-\x1f]/g, "");
    cookie = cookies.map((c) => `${clean(c.name)}=${clean(c.value)}`).join("; ");
  } catch (_e) {
    // pas d'accès cookies pour cette URL : on envoie sans
  }
  return {
    referrer: referrer || undefined,
    cookie: cookie || undefined,
    // Toujours présent (navigator.userAgent est non vide) — voulu : on veut l'UA du navigateur.
    user_agent: navigator.userAgent,
  };
}

// Dernier segment de chemin d'un nom de fichier suggéré par le navigateur.
function basename(name) {
  return String(name || "").split(/[/\\]/).pop() || "";
}

async function getConfig() {
  const {
    token = "",
    port = DEFAULT_PORT,
    autoCapture = true,
    captureTypes = DEFAULT_CAPTURE_TYPES,
  } = await chrome.storage.local.get(["token", "port", "autoCapture", "captureTypes"]);
  return {
    token,
    port: Number(port) || DEFAULT_PORT,
    autoCapture: autoCapture !== false,
    types: parseCaptureTypes(captureTypes),
  };
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
async function sendCapture(url, referrer, filename) {
  if (socket && socket.readyState === WebSocket.OPEN && connected) {
    try {
      const headers = await gatherHeaders(url, referrer);
      socket.send(JSON.stringify(buildCapture(url, { ...headers, filename })));
      return true;
    } catch (_e) {
      return false; // URL invalide
    }
  }
  pending.push({ url, referrer, filename, ts: Date.now() });
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
  if (url) sendCapture(url, info.pageUrl);
});

// Capture automatique : intercepte les téléchargements du navigateur (types listés) et
// les redirige vers FlashGet. N'agit QUE si l'app est connectée (sinon on laisse le
// navigateur télécharger pour ne rien perdre).
chrome.downloads.onDeterminingFilename.addListener((item) => {
  (async () => {
    if (!connected) return; // app injoignable : ne pas intercepter
    const cfg = await getConfig();
    const url = item.finalUrl || item.url;
    if (!shouldCapture({ url, filename: item.filename }, { autoCapture: cfg.autoCapture, types: cfg.types })) {
      return; // type non listé / schéma non récupérable : laisser le navigateur
    }
    try {
      await chrome.downloads.cancel(item.id);
      await chrome.downloads.erase({ id: item.id });
    } catch (_e) {
      return; // annulation impossible : ne pas double-télécharger
    }
    sendCapture(url, item.referrer, basename(item.filename));
  })();
  // Listener synchrone : on n'appelle pas suggest() (le navigateur garde son nom par défaut
  // pour les downloads non capturés ; les capturés sont annulés ci-dessus).
});

// Messages depuis le popup.
chrome.runtime.onMessage.addListener((req, _sender, sendResponse) => {
  if (req.cmd === "status") {
    sendResponse({ connected });
    return false;
  }
  if (req.cmd === "send" && req.url) {
    (async () => {
      let referrer;
      try {
        const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
        referrer = tab && tab.url;
      } catch (_e) {
        // pas d'onglet : referer absent
      }
      const ok = await sendCapture(req.url, referrer);
      sendResponse({ ok });
    })();
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
