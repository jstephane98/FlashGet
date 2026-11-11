# Extension FlashGet (Chromium, Manifest V3)

Envoie les téléchargements du navigateur à l'application FlashGet : capture automatique, clic droit, et bouton « Télécharger la vidéo de cette page ». Compatible Chrome, Edge, Brave et Chromium.

## Installation (mode développeur)

1. Lancer FlashGet (le serveur local écoute toujours sur `127.0.0.1:15151`, le port n'est pas réglable).
2. Ouvrir `chrome://extensions` (ou `edge://extensions`, `brave://extensions`).
3. Activer le **mode développeur**.
4. Cliquer sur **Charger l'extension non empaquetée** et choisir ce dossier `extension/`. Dans l'application, Paramètres → Extension navigateur → **Ouvrir le dossier** y mène directement.

## Jeton

1. Dans FlashGet : Paramètres → Extension navigateur → **Copier** le jeton de connexion.
2. Dans l'extension : **Options** → coller le jeton → **Enregistrer**.
3. La page d'options affiche l'état de la connexion, et la fenêtre de l'extension passe à « Connecté ».

Si le jeton est régénéré dans FlashGet, la fenêtre indique « Le jeton ne correspond plus » : collez le nouveau jeton dans les options.

## Usage

- **Capture automatique** : les téléchargements dont l'extension figure dans la liste « Types capturés » (page d'options) sont repris par FlashGet. Si FlashGet n'est pas connecté, le navigateur télécharge normalement.
- **Clic droit** sur un lien, une image, une vidéo ou un son → « Télécharger avec FlashGet ».
- **Fenêtre de l'extension** : « Télécharger la vidéo de cette page », envoi d'une URL, et l'interrupteur « Capture automatique » du site courant.

## Sites ignorés

L'interrupteur « Capture automatique » de la fenêtre ajoute ou retire le site courant de la liste des sites ignorés. Cette liste est la même que dans FlashGet (Paramètres → Extension navigateur) et se synchronise dans les deux sens.

- Un site ignoré couvre aussi ses sous-domaines : ignorer `drive.google.com` ignore `docs.drive.google.com`, mais pas `google.com`.
- Il bloque seulement la capture automatique : le clic droit et les boutons de la fenêtre continuent de fonctionner.
- La liste est gardée en cache dans le navigateur, et reste active si FlashGet est fermé.

## Limites connues

- Le service worker MV3 se met en veille après environ 30 s d'inactivité ; une alarme le réveille toutes les 30 s pour garder la connexion. Une capture demandée pendant la reconnexion est mise en file (60 s au plus) puis envoyée.
- Un envoi échoué par le clic droit n'est pas notifié ; utilisez la fenêtre de l'extension pour un retour visuel.
