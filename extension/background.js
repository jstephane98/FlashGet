import { buildCapture } from "./buildCapture.js";
import { shouldCapture, parseCaptureTypes, DEFAULT_CAPTURE_TYPES } from "./capture-rules.js";
import { isHlsUrl, isHlsContentType, isDashUrl, isDashContentType, dedupeStreams } from "./media-rules.js";

const DEFAULT_PORT = 15151;
const KEEPALIVE_ALARM = "flashget-keepalive";
const PENDING_TTL_MS = 60000; // une capture en file de plus de 60 s est abandonnée
let socket = null;
let connected = false;
let reconnectTimer = null;
let connecting = false;
let pending = []; // captures en attente d'une connexion authentifiée : [{ url, referrer, filename, ts }]

// Flux HLS détectés par onglet : Map<tabId, [{ url, ts }]>
const detectedByTab = new Map();

function recordStream(tabId, url) {
  if (tabId < 0) return;
  const list = detectedByTab.get(tabId) || [];
  const next = dedupeStreams([...list, { url, ts: Date.now() }]);
  detectedByTab.set(tabId, next.slice(-20)); // borne anti-accumulation
}

chrome.webRequest.onBeforeRequest.addListener(
  (details) => {
    if (isHlsUrl(details.url) || isDashUrl(details.url)) recordStream(details.tabId, details.url);
  },
  { urls: ["http://*/*", "https://*/*"] },
);

chrome.webRequest.onHeadersReceived.addListener(
  (details) => {
    const ct = (details.responseHeaders || []).find(
      (h) => h.name.toLowerCase() === "content-type",
    );
    if (ct && (isHlsContentType(ct.value) || isDashContentType(ct.value))) recordStream(details.tabId, details.url);
  },
  { urls: ["http://*/*", "https://*/*"] },
  ["responseHeaders"],
);

// Purge l'entrée d'un onglet à la navigation (nouvelle URL) et à sa fermeture.
chrome.tabs.onUpdated.addListener((tabId, changeInfo) => {
  if (changeInfo.url) detectedByTab.delete(tabId);
});
chrome.tabs.onRemoved.addListener((tabId) => detectedByTab.delete(tabId));

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
      socket.send(JSON.stringify(buildCapture(it.url, { ...headers, filename: it.filename, kind: it.kind, fallback_url: it.fallback_url, fallback_kind: it.fallback_kind })));
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

// Envoie une capture HLS ou DASH. Comme sendCapture mais marque le type média.
async function sendMedia(url, referrer, title) {
  const kind = isDashUrl(url) ? "dash" : "hls";
  if (socket && socket.readyState === WebSocket.OPEN && connected) {
    try {
      const headers = await gatherHeaders(url, referrer);
      socket.send(JSON.stringify(buildCapture(url, { ...headers, kind, filename: title })));
      return true;
    } catch (_e) {
      return false;
    }
  }
  pending.push({ url, referrer, kind, filename: title, ts: Date.now() });
  if (pending.length > 20) pending.shift();
  connect();
  return true;
}

// Envoie la page courante vers l'extracteur yt-dlp (kind "page").
async function sendPage(url, referrer) {
  if (socket && socket.readyState === WebSocket.OPEN && connected) {
    try {
      const headers = await gatherHeaders(url, referrer);
      socket.send(JSON.stringify(buildCapture(url, { ...headers, kind: "page" })));
      return true;
    } catch (_e) {
      return false;
    }
  }
  pending.push({ url, referrer, kind: "page", ts: Date.now() });
  if (pending.length > 20) pending.shift();
  connect();
  return true;
}

// Bouton vidéo intelligent : envoie l'URL de page (yt-dlp d'abord côté app) + un flux
// HLS/DASH sniffé en repli (kind "smart").
async function sendSmart(url, referrer, fallbackUrl) {
  const extras = { kind: "smart" };
  if (fallbackUrl) {
    extras.fallback_url = fallbackUrl;
    extras.fallback_kind = isDashUrl(fallbackUrl) ? "dash" : "hls";
  }
  if (socket && socket.readyState === WebSocket.OPEN && connected) {
    try {
      const headers = await gatherHeaders(url, referrer);
      socket.send(JSON.stringify(buildCapture(url, { ...headers, ...extras })));
      return true;
    } catch (_e) {
      return false;
    }
  }
  pending.push({ url, referrer, ...extras, ts: Date.now() });
  if (pending.length > 20) pending.shift();
  connect();
  return true;
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
    } catch (_e) {
      return; // annulation impossible : laisser le navigateur, ne pas double-télécharger
    }
    // L'annulation a réussi : le download navigateur est arrêté, on prend le relais même si
    // le nettoyage de l'historique échoue (best-effort, ne doit pas faire perdre la capture).
    try {
      await chrome.downloads.erase({ id: item.id });
    } catch (_e) {
      // nettoyage best-effort
    }
    sendCapture(url, item.referrer, basename(item.filename)).catch(() => {});
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
  if (req.cmd === "listMedia") {
    const list = detectedByTab.get(req.tabId) || [];
    sendResponse({ streams: list });
    return false;
  }
  if (req.cmd === "sendMedia" && req.url) {
    (async () => {
      let referrer;
      let title;
      try {
        const tab = await chrome.tabs.get(req.tabId);
        referrer = tab && tab.url;
        title = tab && tab.title; // titre de la page = nom de fichier proposé
      } catch (_e) {
        // pas de referer / titre : Rust retombera sur "video"
      }
      const ok = await sendMedia(req.url, referrer, title);
      sendResponse({ ok });
    })();
    return true; // réponse asynchrone
  }
  if (req.cmd === "sendPage" && req.url) {
    (async () => {
      let referrer;
      try {
        const tab = await chrome.tabs.get(req.tabId);
        referrer = tab && tab.url;
      } catch (_e) {
        // pas de referer
      }
      const ok = await sendPage(req.url, referrer);
      sendResponse({ ok });
    })();
    return true; // réponse asynchrone
  }
  if (req.cmd === "smartDownload" && req.tabId != null) {
    (async () => {
      let url, referrer;
      try {
        const tab = await chrome.tabs.get(req.tabId);
        url = tab && tab.url;
        referrer = url;
      } catch (_e) {
        // pas d'onglet
      }
      if (!url) { sendResponse({ ok: false }); return; }
      const streams = detectedByTab.get(req.tabId) || [];
      const fallbackUrl = streams.length > 0 ? streams[0].url : undefined;
      const ok = await sendSmart(url, referrer, fallbackUrl);
      sendResponse({ ok });
    })();
    return true; // réponse asynchrone
  }
  return false;
});

// Connexion initiale + keepalive au (ré)veil du service worker.
ensureKeepalive();
connect();
