const dot = document.getElementById("dot");
const statusText = document.getElementById("statusText");
const connectedView = document.getElementById("connectedView");
const disconnectedView = document.getElementById("disconnectedView");
const toast = document.getElementById("toast");

function render(connected) {
  dot.className = "dot " + (connected ? "on" : "off");
  statusText.textContent = connected ? "Connecté" : "Non détecté";
  connectedView.classList.toggle("hidden", !connected);
  disconnectedView.classList.toggle("hidden", connected);
}

function refresh() {
  chrome.runtime.sendMessage({ cmd: "status" }, (res) => {
    render(Boolean(res && res.connected));
  });
}

function flash(text) {
  toast.textContent = text;
  setTimeout(() => (toast.textContent = ""), 1500);
}

document.getElementById("sendUrl").addEventListener("click", () => {
  const url = document.getElementById("url").value;
  if (!url.trim()) return;
  chrome.runtime.sendMessage({ cmd: "send", url }, (res) => {
    flash(res && res.ok ? "Envoyé ✓" : "Échec — non connecté");
  });
});

document.getElementById("downloadVideo").addEventListener("click", async () => {
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  if (!tab || !tab.url) return;
  // Flux HLS/DASH détecté sur l'onglet → capture directe (gère aussi le live) ;
  // sinon → yt-dlp sur l'URL de la page (YouTube, Facebook, etc.).
  chrome.runtime.sendMessage({ cmd: "listMedia", tabId: tab.id }, (res) => {
    const streams = (res && res.streams) || [];
    const msg = streams.length > 0
      ? { cmd: "sendMedia", url: streams[0].url, tabId: tab.id }
      : { cmd: "sendPage", url: tab.url, tabId: tab.id };
    chrome.runtime.sendMessage(msg, (r) => {
      flash(r && r.ok ? "Envoyé ✓" : "Échec — non connecté");
    });
  });
});

document.getElementById("openOptions").addEventListener("click", () => {
  chrome.runtime.openOptionsPage();
});

document.getElementById("retry").addEventListener("click", () => {
  chrome.runtime.sendMessage({ cmd: "reconnect" }, () => setTimeout(refresh, 500));
});

refresh();

