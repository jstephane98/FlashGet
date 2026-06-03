const tokenInput = document.getElementById("token");
const portInput = document.getElementById("port");
const saved = document.getElementById("saved");

async function load() {
  const { token = "", port = 15151 } = await chrome.storage.local.get(["token", "port"]);
  tokenInput.value = token;
  portInput.value = port;
}

document.getElementById("save").addEventListener("click", async () => {
  await chrome.storage.local.set({
    token: tokenInput.value.trim(),
    port: Number(portInput.value) || 15151,
  });
  saved.textContent = "Enregistré ✓";
  setTimeout(() => (saved.textContent = ""), 1500);
  // Forcer une reconnexion avec le nouveau jeton/port.
  chrome.runtime.sendMessage({ cmd: "reconnect" });
});

load();
