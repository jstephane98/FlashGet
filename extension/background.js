import { buildCapture } from "./buildCapture.js";

const DEFAULT_PORT = 15151;
let socket = null;
let connected = false;
let reconnectTimer = null;

async function getConfig() {
  const { token = "", port = DEFAULT_PORT } = await chrome.storage.local.get(["token", "port"]);
  return { token, port: Number(port) || DEFAULT_PORT };
}

function setConnected(value) {
  connected = value;
  // Pastille verte/grise sur l'icône.
  chrome.action.setBadgeText({ text: value ? "●" : "" });
  chrome.action.setBadgeBackgroundColor({ color: value ? "#22c55e" : "#9ca3af" });
}

async function connect() {
  const { token, port } = await getConfig();
  if (socket && (socket.readyState === WebSocket.OPEN || socket.readyState === WebSocket.CONNECTING)) {
    return;
  }
  try {
    socket = new WebSocket(`ws://127.0.0.1:${port}`);
  } catch (_e) {
    scheduleReconnect();
    return;
  }
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

// Envoie une capture ; (re)connecte si nécessaire. Renvoie true si l'envoi a pu partir.
async function sendCapture(url) {
  if (!socket || socket.readyState !== WebSocket.OPEN || !connected) {
    connect();
    return false;
  }
  socket.send(JSON.stringify(buildCapture(url)));
  return true;
}

// Menu contextuel sur liens/images/vidéos/audio.
chrome.runtime.onInstalled.addListener(() => {
  chrome.contextMenus.create({
    id: "flashget-capture",
    title: "Télécharger avec FlashGet",
    contexts: ["link", "image", "video", "audio"],
  });
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

// Connexion initiale au démarrage du service worker.
connect();
