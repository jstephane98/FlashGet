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

document.getElementById("sendTab").addEventListener("click", async () => {
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  if (!tab || !tab.url) return;
  chrome.runtime.sendMessage({ cmd: "send", url: tab.url }, (res) => {
    flash(res && res.ok ? "Envoyé ✓" : "Échec — non connecté");
  });
});

document.getElementById("sendPage").addEventListener("click", async () => {
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  if (!tab || !tab.url) return;
  chrome.runtime.sendMessage({ cmd: "sendPage", url: tab.url, tabId: tab.id }, (res) => {
    flash(res && res.ok ? "Envoyé ✓" : "Échec — non connecté");
  });
});

document.getElementById("openOptions").addEventListener("click", () => {
  chrome.runtime.openOptionsPage();
});

document.getElementById("retry").addEventListener("click", () => {
  chrome.runtime.sendMessage({ cmd: "reconnect" }, () => setTimeout(refresh, 500));
});

refresh();

async function refreshMedia() {
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  if (!tab) return;
  chrome.runtime.sendMessage({ cmd: "listMedia", tabId: tab.id }, (res) => {
    const streams = (res && res.streams) || [];
    const section = document.getElementById("mediaSection");
    const list = document.getElementById("mediaList");
    list.innerHTML = "";
    section.classList.toggle("hidden", streams.length === 0);
    streams.forEach((s, i) => {
      const btn = document.createElement("button");
      btn.textContent = `Télécharger la vidéo ${streams.length > 1 ? i + 1 : ""}`.trim();
      btn.addEventListener("click", () => {
        chrome.runtime.sendMessage({ cmd: "sendMedia", url: s.url, tabId: tab.id }, (r) => {
          flash(r && r.ok ? "Envoyé ✓" : "Échec — non connecté");
        });
      });
      list.appendChild(btn);
    });
  });
}

refreshMedia();
