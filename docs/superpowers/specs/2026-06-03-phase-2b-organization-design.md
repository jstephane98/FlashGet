# Phase 2b — Organisation (catégories, recherche/filtres, historique) — Design

**Date :** 2026-06-03
**Statut :** Validé (design) — en attente de relecture finale avant plan d'implémentation
**Projet :** FlashGet (clone d'IDM) — voir [spec produit](2026-06-02-idm-clone-design.md)
**Pré-requis :** Phase 1 (cœur) + Phase 2a (file d'attente, priorités, throttling) — fusionnées dans `main`.

## 1. Objectif et périmètre

Phase 2b complète la gestion des téléchargements par l'**organisation** :
- **Catégories automatiques** : ensemble prédéfini (Vidéos, Documents, Programmes, Archives, Musique, Autre), chacune avec une liste d'extensions et un **dossier de destination configurable**.
- **Routage à l'ajout** : le fichier est catégorisé via son extension et téléchargé **directement** dans le dossier de sa catégorie (pas de déplacement).
- **Recherche & filtres** : recherche live par nom + onglets de catégories + filtre par statut.
- **Historique / actions** : la liste fait office d'historique ; actions par ligne « Ouvrir le fichier », « Ouvrir le dossier », « Retirer de la liste » (sans effacer le fichier), et « Vider les terminés ».

**Hors périmètre** : catégories personnalisées créées par l'utilisateur (l'ensemble est fixe en 2b) ; déplacement après complétion ; intégration navigateur (P3) ; vidéo/streaming (P4).

## 2. Approche d'architecture (validée)

Catégoriseur **pur** côté Rust (testable comme `compute_segments`/`pick_to_start`) ; config des dossiers persistée dans la table `settings` existante ; **filtrage côté frontend** (recherche/onglet/statut sont locaux, pas de requêtes backend). Le backend fournit la catégorie et la config des dossiers ; il route à l'ajout.

## 3. Composants

### 3.1 `categorizer.rs` (nouveau, `download-core`)
```
pub fn categorize(file_name: &str) -> Category
```
- Extrait l'extension (dernier segment après le dernier `.`), en **minuscules**.
- Table par défaut (extensions → catégorie) :
  - **Video** : mp4, mkv, avi, mov, webm, flv, wmv, m4v, mpg, mpeg, ts
  - **Document** : pdf, doc, docx, xls, xlsx, ppt, pptx, txt, odt, ods, csv, epub, rtf
  - **Program** : exe, msi, dmg, pkg, deb, rpm, apk, appimage
  - **Archive** : zip, rar, 7z, tar, gz, bz2, xz, zst, iso
  - **Music** : mp3, flac, wav, aac, ogg, m4a, wma, opus
  - **Other** : tout le reste (extension inconnue ou absente)
- Cas multi-points : `archive.tar.gz` → extension `gz` → Archives. (On ne regarde que la dernière extension ; `.tar.gz` est couvert car `gz` ∈ Archive.)

### 3.2 `model.rs` (ajouts)
- `enum Category { Video, Document, Program, Archive, Music, Other }` — `#[serde(rename_all = "lowercase")]`, `#[default] Other`.
- `Download` gagne `#[serde(default)] pub category: Category`.

### 3.3 `storage.rs` (ajouts)
- Migration additive idempotente : `ALTER TABLE downloads ADD COLUMN category TEXT NOT NULL DEFAULT 'other'` (erreur « duplicate column » ignorée).
- `upsert`/`list`/`get_by_id` round-trip `category` ; helpers `category_to_str`/`category_from_str`.
- Dossiers par catégorie dans la table `settings` (clés `dest.video`, `dest.document`, … ; valeur = chemin, vide = défaut Téléchargements) :
  - `get_category_dirs() -> CategoryDirs` (map catégorie → Option<String>).
  - `set_category_dir(category, path: Option<String>)`.
- `remove_from_list(id)` = `delete` de la ligne uniquement (le fichier n'est jamais touché). *(Implémenté via le `delete` existant ; la distinction d'avec `cancel` est dans la couche commandes.)*
- `clear_completed()` = `DELETE FROM downloads WHERE status='completed'`.

### 3.4 Couche Tauri (`commands.rs`)
- `add_download` (modifié) : sonde → `file_name` assaini → `categorize(file_name)` → dossier = `get_category_dirs()[cat]` si non vide, sinon `download_dir()` ; `create_dir_all(dossier)` ; garde de confinement anti-traversal ; persiste `category`.
- Nouvelles commandes :
  - `get_category_dirs()` / `set_category_dir(category, path)`.
  - `remove_from_list(id)` : `storage.delete(&id)` **sans** effacer le fichier ; retire aussi le handle s'il existe ; **ne stoppe pas** le moteur (réservé aux non-actifs côté UI).
  - `clear_completed()`.
  - `open_file(id)` : ouvre `dest_path` via `tauri-plugin-opener`.
  - `open_folder(id)` : révèle/ouvre le dossier parent de `dest_path`.
- Toute nouvelle tâche éventuelle utilise **`tauri::async_runtime::spawn`** (rappel du correctif 2a — jamais `tokio::spawn` depuis une commande).

### 3.5 Frontend
- `categorize`/filtrage : un helper pur **`filterDownloads(list, {query, category, status})`** (testé Vitest) applique recherche (sous-chaîne insensible à la casse sur `file_name`), onglet catégorie (`"all"` ou une `Category`), et statut (`"all"` ou un `DownloadStatus`).
- `types.ts` : `Category`, `Download.category`, `CategoryDirs`. `api.ts` : `getCategoryDirs`, `setCategoryDir`, `removeFromList`, `clearCompleted`, `openFile`, `openFolder`.
- Composants : `CategoryTabs.tsx` (onglets), `RowMenu.tsx` (menu ⋯ d'actions), `CategoryPill.tsx` ; `Toolbar.tsx` étendu (recherche + filtre statut) ; `DownloadRow.tsx`/`DownloadTable.tsx` (colonne Cat. + menu) ; `SettingsDialog.tsx` (section Dossiers par catégorie, sélection via le dialogue de dossier Tauri) ; `App.tsx` (état des filtres, application de `filterDownloads`).
- Choix de dossier : via le plugin `dialog` de Tauri (`@tauri-apps/plugin-dialog` `open({directory:true})`) — **à ajouter** (plugin + permission capability).

## 4. Flux & comportements

**Ajout** : catégorisation → dossier de la catégorie (ou Téléchargements) → création du dossier → téléchargement direct dedans → persistance de `category`.

**Actions ligne :**
- *Ouvrir le fichier* (actif si `Completed`) → `open_file`.
- *Ouvrir le dossier* → `open_folder`.
- *Retirer de la liste* (`remove_from_list`) → oublie l'entrée, **garde** le fichier.
- *Annuler* (`cancel`, existant) → stoppe + **efface** le fichier partiel.
- *Vider les terminés* (toolbar/menu) → `clear_completed`.

**Filtrage (frontend, live)** : liste affichée = `filterDownloads(downloads, {query, category, status})`. Les events de progression mettent à jour `downloads` ; le filtre s'applique au rendu.

**Cas limites :** dossier catégorie inaccessible → `Error` clair ; changement de dossier → nouveaux ajouts seulement ; ouverture d'un fichier déplacé/supprimé → échec propre (message), pas de crash ; migration → anciens `Download` en `Other`.

## 5. Interface (layout B + ajouts — validée)

- Toolbar : recherche par nom + filtre statut (menu déroulant) + ⚙ (en plus de + Ajouter / ⏸ Tout / ▶ Tout).
- Onglets de catégories sous la toolbar (Tous + une par catégorie).
- Colonne « Cat. » (pastille) dans le tableau.
- Menu d'actions ⋯ par ligne (Ouvrir fichier / Ouvrir dossier / Retirer de la liste / Annuler).
- Réglages : section « Dossiers par catégorie » (chemin + Parcourir… par catégorie ; vide = Téléchargements).

## 6. Stratégie de tests

- **`categorizer::categorize`** (TDD, pur) : chaque catégorie via extensions représentatives ; insensible à la casse ; inconnue/sans extension → `Other` ; `archive.tar.gz` → Archive.
- **`storage`** (TDD) : round-trip `category` ; migration sans la colonne ; `get_category_dirs`/`set_category_dir` ; `clear_completed` ne retire que les `Completed` ; `delete` (remove_from_list) ne touche pas au fichier (vérifié logiquement — storage ne gère pas le fichier).
- **Frontend (Vitest)** : `filterDownloads` (nom partiel, onglet, statut, combinaisons, casse) ; rendu pastille + menu selon statut.
- **Manuel (smoke-test)** : routage par catégorie réel ; Ouvrir fichier/dossier ; Retirer vs Annuler ; choix de dossier via le dialogue.

## 7. Décisions reportées / hors 2b

- Catégories personnalisées (création/édition par l'utilisateur).
- Déplacement post-complétion ; règles avancées (par domaine d'URL, par type MIME).
- Backlog technique 2a toujours ouvert : fast-path lock-free du limiteur en illimité ; persistance périodique du statut `downloading` ; test e2e d'erreur.
