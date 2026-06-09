# Refonte UI « Cockpit sombre » — app + extension — Design

**Date :** 2026-06-09
**Statut :** validé (design system approuvé via maquettes)
**Module :** FlashGet (app Tauri React/TS + extension Chromium MV3)

## Intention

Donner à FlashGet une identité visuelle forte et cohérente, en remplaçant l'UI générique
« indigo sur blanc » par une direction **« Cockpit sombre »** (outil pro / centre de contrôle),
déclinée sur l'app **et** l'extension. Implémentation guidée par la skill `frontend-design`.

## Direction retenue (validée en maquettes)

- **Thème :** sombre (cockpit), fond ardoise.
- **Accent :** ambre / or (`#f59e0b`), dégradé `#fbbf24 → #f59e0b` pour les CTA.
- **Typo :** **Archivo** (interface, titres, wordmark) + **JetBrains Mono** (chiffres : débit, taille, %, ports, jeton).
- **Densité :** confortable (lignes hautes, respiration).

## Design tokens (contrat d'implémentation)

À exposer en variables CSS (`:root`) ET dans `tailwind.config` (theme.extend) pour l'app ; en CSS pur pour l'extension.

### Couleurs
| Rôle | Valeur |
|---|---|
| Fond app (base) | `#0f172a` (slate-900) |
| Fond profond (bords/chrome, backdrop) | `#020617` (slate-950) |
| Surface / panneau (lignes, inputs, dialogues) | `#1e293b` (slate-800) |
| Bordure | `#1e293b` / `#334155` (slate-800/700) |
| Texte primaire | `#e2e8f0` (slate-200) |
| Texte secondaire | `#94a3b8` (slate-400) |
| Texte atténué | `#64748b` (slate-500) |
| **Accent (base)** | `#f59e0b` (amber-500) |
| Accent clair | `#fbbf24` (amber-400) |
| Accent texte sur fond sombre | `#fcd34d` (amber-300) |
| Texte sur accent | `#3b1d00` (brun foncé) |
| Dégradé CTA | `linear-gradient(160deg,#fbbf24,#f59e0b)` |

### Couleurs de statut (pastille + barre + libellé)
| Statut | Couleur |
|---|---|
| En cours (downloading) | accent ambre `#f59e0b` (pastille avec halo `box-shadow:0 0 8px`) |
| Terminé / connecté | émeraude `#34d399` (barre `#10b981`) |
| En file (queued) | slate `#475569` / barre `#334155` |
| En pause | ambre atténué `#fbbf24` (sans halo) |
| Erreur | rouge `#f87171` / `#ef4444` |
| Enregistrement (live) | rouge `#ef4444` (point clignotant léger) |

### Typo, rayons, ombres, motion
- **Polices auto-hébergées** (offline / CSP) : Archivo (500/700/800) et JetBrains Mono (500/700) en `woff2` locaux + `@font-face`. **Pas de CDN Google** (l'app Tauri et l'extension MV3 n'ont pas d'accès réseau garanti / CSP). Fallbacks : `Archivo, ui-sans-serif, system-ui, sans-serif` et `'JetBrains Mono', ui-monospace, monospace`.
- **Rayons :** boutons/inputs `10–11px`, panneaux/dialogues `12–14px`, pastilles `999px`.
- **Ombres :** dialogues/popup `0 20px 50px rgba(0,0,0,.5)` ; CTA accent `0 6px 18px rgba(245,158,11,.35)`.
- **Motion (sobre, CSS d'abord) :** révélation échelonnée des lignes au chargement (stagger via `animation-delay`), transitions de la barre de progression (`width .3s`), hover discret (légère élévation/bordure accent). Pas de bibliothèque d'animation ajoutée (YAGNI) sauf si déjà présente.

## Application par surface

### App (React + Tailwind v3)
1. **Thème & polices :** `tailwind.config.js` → `theme.extend.colors` (palette ci-dessus, p.ex. `bg`, `surface`, `accent`, `accent-soft`…) + `fontFamily.sans = Archivo`, `fontFamily.mono = JetBrains Mono`. `src/index.css` → `@font-face` (woff2 sous `src/assets/fonts/`), fond sombre de base sur `html,body`, variables CSS. Fichiers fonts ajoutés au repo.
2. **`App.tsx` :** fond sombre, bandeaux erreur (rouge sombre) / info (ambre sombre) ré-habillés.
3. **`Toolbar.tsx` :** CTA « + Ajouter » en dégradé ambre ; « + Lot » en surface ; recherche/sélecteur sombres ; icônes lisibles sur sombre.
4. **`CategoryTabs.tsx` / `CategoryPill.tsx` :** onglets sombres, actif souligné/teinté ambre.
5. **`DownloadTable.tsx` / `DownloadRow.tsx` :** cœur — lignes confortables, nom en Archivo, **valeurs numériques en JetBrains Mono** (débit/taille/%), pastille de statut colorée + barre de progression selon statut, hover discret, sélection.
6. **`PriorityPill.tsx` / `RowMenu.tsx` :** pills sombres ; menu contextuel en surface avec bordure.
7. **Dialogues** (`AddUrlDialog`, `AddBatchDialog`, `SettingsDialog`, `HlsVariantDialog`) : backdrop `#020617cc`, panneau `#0f172a`/`#1e293b`, bordure, titres Archivo, champs sombres, bouton primaire ambre. La case « Démarrer avec Windows » et la section extension des Réglages suivent le thème.

### Extension (HTML/CSS/JS)
8. **CSS partagé** `extension/ui.css` (tokens cockpit + `@font-face` woff2 sous `extension/fonts/`) inclus par `popup.html` et `options.html`.
9. **`popup.html` :** en-tête logo + wordmark + statut (point vert « Connecté »), CTA vidéo en dégradé ambre, séparateur « ou un lien direct », champ coller-URL, pied monospace (`host:port` + état jeton).
10. **`options.html` :** mêmes fond/accent/typo (jeton, port).

## Contraintes / non-régression

- **Comportement inchangé** : file, scheduler, tray, autostart, capture, yt-dlp — aucun changement de logique. **Refonte purement visuelle** (className/CSS/markup de présentation), pas de renommage de commandes ni d'API.
- **Tests Vitest** : les tests existants vérifient des **libellés et structures** (`DownloadRow.test`, `AddBatchDialog.test`, `HlsVariantDialog.test`). **Conserver les textes/`aria-label` existants** (« Ajouter », « Télécharger », « Liens à télécharger », « Nom du fichier »…) pour ne pas les casser. Si un changement de markup casse un test, **mettre à jour le test** dans la même tâche (le test suit la présentation, sans changer le comportement vérifié).
- **Lisibilité / contraste** : viser un contraste AA sur le texte (le sombre + ambre le permet ; vérifier le texte secondaire).
- **Icône** : cohérente avec l'icône FlashGet récente (flèche + ambre) — le wordmark/logo du popup réutilise le motif flèche.

## Hors périmètre (YAGNI)

- Bascule thème clair/sombre (on fait **sombre uniquement**).
- Bibliothèque d'animation (Motion/Framer) — CSS suffit.
- Renommage `productName` `tauri-app` → « FlashGet ».
- Nouvelles fonctionnalités UI (le périmètre = re-habillage des écrans existants).

## Tests / vérification

Visuelle (maquettes déjà validées + rendu réel remonté pendant l'implémentation) + `npm test` (Vitest) vert (tests présentation mis à jour si besoin) + `npx tsc --noEmit` + l'app se lance (`tauri dev`). Pas de test unitaire de style.

## Séquencement

Branche `phase-redesign` (déjà créée depuis `main`). Implémentation guidée par `frontend-design`, en tâches (tokens/polices → composants app → dialogues → extension), puis rebuild de l'installateur et merge.
