# Phase 2a — Moteur de gestion (file d'attente, concurrence, priorités, throttling) — Design

**Date :** 2026-06-03
**Statut :** Validé (design) — en attente de relecture finale avant plan d'implémentation
**Projet :** FlashGet (clone d'IDM) — voir [spec produit](2026-06-02-idm-clone-design.md)
**Pré-requis :** Phase 1 (cœur multi-segments + reprise + UI tableau) — fusionnée dans `main`.

## 1. Objectif et périmètre

La Phase 2 a été scindée en **2a** (ce document) et **2b** (catégories, recherche/filtres, historique — plus tard).

**2a livre le moteur de gestion des téléchargements :**
- File d'attente avec **limite de téléchargements simultanés** (défaut 3).
- **Démarrage automatique** : à l'ajout, démarre si un slot est libre, sinon met « En file » ; démarre automatiquement quand un slot se libère.
- **Priorités** : niveaux `Haute` / `Normale` / `Basse` ; à la libération d'un slot, on démarre la plus haute priorité en file (FIFO à niveau égal).
- **Limiteur de bande passante global** : une seule limite de vitesse totale (0 = illimitée), partagée entre tous les téléchargements/segments actifs.
- **Réglages persistants** : max simultanés + vitesse max, modifiables via un panneau Réglages.
- Confort : **pause tout / reprendre tout**.

**Hors périmètre 2a** (Phase 2b et au-delà) : catégories automatiques + dossiers de destination, recherche/filtres, historique, limite de vitesse par téléchargement.

## 2. Approche d'architecture (validée)

**Scheduler centralisé dans le cœur Rust avec une fonction de décision pure.** La logique « qui démarrer » est une fonction pure testable sans Tauri ni réseau (comme `compute_segments` en Phase 1). La couche Tauri *pilote* : à chaque événement pouvant libérer/occuper un slot, elle appelle la décision et démarre les téléchargements élus. Le throttling est un *rate limiter* (token bucket) partagé, injecté dans la boucle de chunks du moteur.

Alternatives écartées : logique dispersée dans la couche commandes (non testable, sujette aux races) ; tout en callbacks d'événements (logique éparpillée).

## 3. Composants du cœur (`download-core`)

### 3.1 `model.rs` (ajouts)
- Nouvel enum `Priority { High, Normal, Low }` (sérialisé en minuscules pour le front), défaut `Normal`.
- `Download` gagne `priority: Priority`.
- Nouveau type `Settings { max_concurrent: u32, max_speed_bps: u64 }`. `max_speed_bps == 0` ⇒ illimité. Défauts : `max_concurrent = 3`, `max_speed_bps = 0`.

### 3.2 `scheduler.rs` (nouveau) — décision pure
```
pub fn pick_to_start(downloads: &[Download], max_concurrent: u32) -> Vec<String>
```
- Compte les `Downloading` actifs ; `slots = max_concurrent.saturating_sub(actifs)`.
- Parmi les `Queued`, trie par priorité (High > Normal > Low) puis par ordre d'arrivée stable (l'ordre du slice = ordre d'ajout, garanti par un `ORDER BY rowid` au chargement) ; renvoie les `slots` premiers **ids**.
- Ne renvoie jamais un id déjà actif, `Paused`, `Completed` ou `Error` ⇒ **idempotent**.
- Si `slots == 0` ou aucun `Queued`, renvoie `vec![]`.

### 3.3 `rate_limiter.rs` (nouveau) — throttling global
- Token bucket asynchrone partagé : `RateLimiter` avec `async fn acquire(&self, n: u64)` qui ne rend la main que lorsque `n` octets de budget sont disponibles. Budget rechargé proportionnellement au temps écoulé, plafonné à une petite réserve (burst borné).
- `max_bps == 0` ⇒ **pass-through** (retourne immédiatement, aucun coût).
- `fn set_rate(&self, max_bps: u64)` ajuste le débit **à chaud**.
- **Horloge injectable** (trait `Clock` avec une implémentation réelle et une implémentation de test) pour des tests déterministes sans `sleep` réel.
- Partagé via `Arc` entre tous les segments de tous les téléchargements ⇒ limite globale.

### 3.4 `engine.rs` (changement)
- `download(...)` reçoit un paramètre supplémentaire `limiter: Arc<RateLimiter>`.
- Dans la boucle de chunks de chaque segment (et du chemin mono-flux), avant de comptabiliser/écrire un chunk : `limiter.acquire(chunk.len() as u64).await`.
- Effet : le débit cumulé respecte le plafond, réparti naturellement entre segments concurrents. En mode illimité, aucun changement de comportement observable.
- Les tests Phase 1 existants passent un `RateLimiter` illimité (helper `RateLimiter::unlimited()`).

### 3.5 `storage.rs` (ajouts)
- **Migration additive** au démarrage : `ALTER TABLE downloads ADD COLUMN priority TEXT NOT NULL DEFAULT 'normal'` (gardé idempotent — ignorer l'erreur « duplicate column » pour les bases déjà migrées) ; `CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL)`.
- Round-trip de `priority` dans `upsert`/`list` ; `list()` ordonné par `rowid` (ordre d'ajout stable, requis par le scheduler).
- `get_settings() -> Settings` (valeurs par défaut si absentes) et `set_settings(&Settings)`.
- `get_by_id(id) -> Option<Download>` (remplace les `list()`+`find` de la couche commandes — backlog Phase 2).
- `set_priority(id, Priority)`.

## 4. Flux & comportements

**Ré-évaluation** : déclenchée par ajout, fin (Completed/Error), pause, reprise, annulation, changement de `max_concurrent`. Elle appelle `pick_to_start(list, max_concurrent)` puis démarre les ids renvoyés (passage `Queued` → `Downloading` + lancement de la tâche moteur).

**Statuts :**
- Ajout ⇒ `Queued`, puis ré-évaluation (démarre si slot libre). *(Change le comportement Phase 1 qui démarrait inconditionnellement.)*
- `Paused` = arrêt **volontaire** : sort du compte des actifs et **ne redémarre jamais seul**.
- `Queued` = en attente de slot : **démarre automatiquement**.
- Reprise manuelle d'un `Paused` ⇒ repasse `Queued` puis ré-évaluation.
- Fin ⇒ `Completed` / `Error` ⇒ libère un slot ⇒ ré-évaluation.

Cette distinction **Paused vs Queued** est le point de comportement central : un téléchargement mis en pause ne repart pas tout seul.

**Réduction de `max_concurrent` à chaud** (ex. 3→1) : non-destructif — les actifs en cours **continuent** ; on cesse simplement d'en démarrer de nouveaux jusqu'à repasser sous la limite.

**Changement de vitesse à chaud** : `RateLimiter::set_rate` ; aucun redémarrage des téléchargements en cours.

### Couche Tauri (changements)
- `AppState` détient désormais : le `RateLimiter` partagé (`Arc`), **un seul `reqwest::Client` mutualisé** (backlog Phase 2), et une fonction interne `reevaluate(&AppHandle)` centralisant la ré-évaluation.
- `add_download` : crée en `Queued`, persiste, puis `reevaluate`.
- `start_download` (reprise) : repasse l'id en `Queued`, `reevaluate`.
- `pause_download` / `cancel_download` : comme Phase 1, puis `reevaluate` (un slot peut s'être libéré).
- Nouveau `set_priority(id, level)` : persiste, `reevaluate`.
- Nouveau `get_settings()` / `update_settings(max_concurrent, max_speed_bps)` : persiste, applique (`limiter.set_rate`), `reevaluate`.
- Nouveau `pause_all()` / `resume_all()`.
- La tâche moteur reçoit le `RateLimiter` partagé et le `Client` mutualisé.

## 5. Interface (layout B + ajouts — validée)

- **Toolbar** : boutons `⏸ Tout` / `▶ Tout`, indicateur de débit global + limite, icône `⚙ Réglages`.
- **Colonne Priorité** : pastille colorée (HAUTE / NORMALE / BASSE) avec menu déroulant par ligne appelant `set_priority`.
- **État « En file (n) »** : position dans la file pour les `Queued`.
- **Panneau Réglages** (modale) : *téléchargements simultanés max* (entier) et *vitesse de téléchargement max* (valeur + unité Ko/s·Mo/s + case « Illimitée »). Boutons Annuler / Enregistrer ⇒ `update_settings`.
- Composants front : nouveau `SettingsDialog.tsx`, `PriorityPill.tsx` ; `Toolbar.tsx` et `DownloadRow.tsx` étendus ; `api.ts`/`types.ts` étendus (Priority, Settings, nouvelles commandes).

## 6. Robustesse & cas limites

- Réduction de `max_concurrent` en cours : non-destructive (cf. §4).
- Échec d'un actif : libère un slot ⇒ démarre le suivant.
- Throttle `0`/illimité : pass-through, aucun blocage ni coût.
- Vitesse changée à chaud : recalcul du token bucket sans toucher aux téléchargements actifs.
- Migration base : colonnes/tables ajoutées si absentes au démarrage ; bases Phase 1 existantes restent valides.
- Idempotence de la ré-évaluation : garantie par la fonction pure (ne renvoie que des `Queued`, jamais un actif).

## 7. Stratégie de tests

- **`scheduler::pick_to_start`** (TDD, pur) : slots libres/pleins ; tri par priorité ; FIFO à niveau égal ; ignore Paused/Completed/Error ; idempotence ; `slots == 0`.
- **`rate_limiter`** (TDD, horloge injectable) : débit respecté sur une fenêtre temporelle simulée ; burst borné ; mode illimité = pass-through ; `set_rate` à chaud.
- **`storage`** (TDD) : round-trip `priority` + `settings` ; migration d'une base sans colonne `priority`/table `settings` ; `get_by_id` ; ordre `list()` stable.
- **Moteur + limiteur** (intégration, horloge contrôlée) : le débit cumulé reste sous le plafond global.
- **Frontend** (Vitest) : affichage pastille priorité ; libellé « En file (n) » ; nouveaux contrôles.

## 8. Décisions reportées / hors 2a

- Catégories automatiques + dossiers de destination, recherche/filtres, historique (Phase 2b).
- Limite de vitesse **par téléchargement** (plus tard, si besoin).
- Persistance périodique du statut `downloading` et tolérance au poisoning des mutex (backlog technique, peut être glissé dans le plan 2a si opportun).
