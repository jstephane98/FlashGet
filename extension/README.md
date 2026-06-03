# Extension FlashGet (Chromium MV3)

Capture manuelle de liens vers l'application FlashGet.

## Charger en non empaqueté (Chrome / Edge)

1. Lancer l'application FlashGet (le serveur local écoute sur `127.0.0.1:15151`).
2. Ouvrir `chrome://extensions` (ou `edge://extensions`).
3. Activer le **mode développeur**.
4. **Charger l'extension non empaquetée** → sélectionner ce dossier `extension/`.
5. Dans l'application : Réglages → 🧩 Extension navigateur → **Copier** le jeton.
6. Dans l'extension : **Options** → coller le jeton → Enregistrer.
7. La pastille de l'icône passe au vert (connecté).

## Usage

- **Clic droit** sur un lien/image/vidéo/audio → « Télécharger avec FlashGet ».
- **Popup** : coller une URL, ou « Envoyer l'onglet courant ».

## Limites connues (3a)

- **Première capture après mise en veille :** le service worker MV3 de Chrome se met en veille après ~30 s d'inactivité, ce qui ferme la connexion locale. La première capture qui suit peut échouer (« Échec — non connecté ») tout en relançant la connexion en arrière-plan ; réessayez une fois, ou utilisez **Réessayer** dans le popup. La mise en file d'attente automatique des captures arrivera en 3b.
- **Menu contextuel sans retour visuel :** un envoi échoué via le clic droit n'est pas notifié (pas de permission `notifications` en 3a). Pour un retour « Envoyé ✓ », utilisez le popup.
