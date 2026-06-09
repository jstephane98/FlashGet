# Remplacement des emoji par des icônes Lucide — Design

**Date :** 2026-06-09
**Statut :** validé (design approuvé)
**Module :** FlashGet (app Tauri React/TS + extension Chromium MV3)

## Intention

Remplacer les **emoji** de l'UI (⏸ ▶ ✕ 🧹 ⚙ 🔍 🧩 ⬇ + emoji de catégories 🎬 📄 ⚙ 🗜 🎵 …)
par un **jeu d'icônes cohérent (Lucide)**, en trait fin et monochrome, qui colle au thème
« Cockpit sombre ». Refonte **purement visuelle** : libellés texte et comportement inchangés.

## Mécanisme

- **App (React + Tailwind) :** dépendance **`lucide-react`**. Chaque emoji → `<Icon size={16} />`
  (14 px pour les boutons de ligne compacts). Les icônes héritent de **`currentColor`** → elles
  prennent la couleur du bouton (ink-soft, ambre…). `strokeWidth` Lucide par défaut (2).
- **Extension (HTML/JS, sans bundler) :** **SVG inline** (mêmes icônes Lucide) dans `popup.html`,
  `stroke="currentColor"` + dimension par CSS. Pas de dépendance ajoutée à l'extension.

## Correspondance emoji → icône Lucide

**Toolbar** (`src/components/Toolbar.tsx`)
| Avant | Après | Bouton |
|---|---|---|
| `+` | `Plus` | « Ajouter » |
| `+` | `ListPlus` | « Lot » |
| ⏸ | `Pause` | « Tout » (pause tout) |
| ▶ | `Play` | « Tout » (reprendre tout) |
| 🔍 | `Search` | dans le champ recherche (placeholder « Rechercher par nom… ») |
| 🧹 | `Trash2` | icône seule, `aria-label`/`title` « Vider les terminés » |
| ⚙ | `Settings` | icône seule, `aria-label`/`title` « Réglages » |

**Catégories** (`CategoryTabs.tsx`, `CategoryPill.tsx`, `SettingsDialog.tsx` liste `CATS`) — l'emoji sort du libellé, remplacé par une icône devant le texte :
| Catégorie | Icône |
|---|---|
| all « Tous » | `LayoutGrid` |
| video « Vidéos » | `Film` |
| document « Documents » | `FileText` |
| program « Programmes » | `MonitorCog` |
| archive « Archives » | `FileArchive` |
| music « Musique » | `Music` |
| other « Autre » | `Folder` |

**Ligne de téléchargement** (`DownloadRow.tsx`)
| Avant | Après | Note |
|---|---|---|
| ⏹ | `Square` | stop (garder `title` « Arrêter (finalise l'enregistrement) ») |
| ⏸ | `Pause` | ajouter `aria-label`/`title` « Mettre en pause » |
| ▶ | `Play` | ajouter `aria-label`/`title` « Reprendre » |
| ✕ | `X` | garder `title` « Annuler (efface le fichier) » |

**Menu de ligne** (`RowMenu.tsx`)
| Avant | Après |
|---|---|
| ⋯ | `MoreHorizontal` (garder `title` « Actions ») |
| 📂 Ouvrir le fichier | `File` + texte |
| 🗂 Ouvrir le dossier | `FolderOpen` + texte |
| 🗑 Retirer de la liste | `Trash2` + texte |

**Réglages** (`SettingsDialog.tsx`)
| Avant | Après |
|---|---|
| ⚙ Réglages (titre) | `Settings` + texte |
| 🧩 Extension navigateur | `Puzzle` + texte |
| ✕ (effacer dossier de catégorie) | `X`, `aria-label`/`title` « Retirer le dossier » |

**Bannières** (`App.tsx`) : les ✕ de fermeture (erreur + info) → `X` avec `aria-label` « Fermer ».

**HlsVariantDialog.tsx** : remplacer tout emoji présent (ex. ✕/coche) par l'icône Lucide
correspondante (`X` / `Check`), au moment de l'implémentation (lire le fichier).

**Extension** (`extension/popup.html`)
| Avant | Après |
|---|---|
| ↓ (logo) | SVG Lucide `Download` (blanc, dans la pastille dégradée) |
| ⬇ « Télécharger la vidéo de cette page » | SVG Lucide `Download` avant le texte |

(SVG `Download` : `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><path d="m7 10 5 5 5-5"/><path d="M12 15V3"/></svg>`.)

## Accessibilité (amélioration au passage)

Tout bouton **icône seule** (🧹, ⚙, ✕ de bannière, pause/reprendre/annuler/stop de ligne, `⋯`)
reçoit un **`aria-label`** (et/ou `title`). Bénéfice double : accessibilité + tests sélectionnables
par **nom accessible** plutôt que par glyphe.

## Périmètre & garanties

- Surfaces : `Toolbar`, `CategoryTabs`, `CategoryPill`, `DownloadRow`, `RowMenu`, `SettingsDialog`,
  `App` (bannières), `HlsVariantDialog`, + extension `popup.html`.
- **Aucun changement de comportement** : on remplace le glyphe par une icône ; les libellés texte
  (« Vidéos », « Ajouter », « Reprendre »…) restent, sans l'emoji.
- **Tests** : à ce jour **aucun test ne cible un emoji** (vérifié) → impact attendu nul. Si un test
  casse malgré tout (markup), le mettre à jour pour cibler le **nom accessible**, sans l'affaiblir.
- **Couleur/taille** : icônes en `currentColor`, ~16 px (14 px boutons de ligne) ; cohérence avec
  le thème ambre/sombre automatique.

## Hors périmètre (YAGNI)

- L'icône de l'app/installateur (sujet séparé déjà en backlog : la passer en ambre).
- Changer les libellés texte ou la disposition.
- Animations d'icônes.

## Tests / vérification

`npx tsc --noEmit` + `npm test` (mis à jour si besoin) + smoke visuel (`tauri dev` + extension).
Pas de test unitaire de style.

## Séquencement

Branche `phase-icons-lucide` depuis `main`. Implémentation guidée par `frontend-design`
(en tâches : dépendance + Toolbar/catégories → ligne + menu → réglages/bannières/dialogue →
extension), puis vérif + (optionnel) rebuild installateur, puis merge.
