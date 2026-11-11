# FlashGet v2 — Phase 4 : file d'attente et planificateur — Plan d'implémentation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal :** livrer les pages « File d'attente » et « Planificateur » de la maquette 04.
- **File d'attente** : liste numérotée, ordre modifiable par glisser-déposer, priorité, retrait, « Suspendre la file », « Tout démarrer maintenant ».
- **Planificateur** : plage horaire et jours, frise 24 h, limite de vitesse pendant la plage, action à la fin de la plage, action quand tout est terminé (veille, arrêt…) avec 60 s pour annuler.

**Architecture :**
- **Moteur** : un rang de file persistant (`queue_rank`) départage les éléments de même priorité. Un indicateur « file suspendue » empêche tout démarrage automatique.
- **Planificateur** : une fonction pure dit si l'on est dans la plage et quand elle recommence. Une tâche Tauri vérifie la plage toutes les 30 s ; à l'entrée, elle passe les éléments « en attente » (`held`) en file et applique la limite de vitesse ; à la sortie, elle applique l'action prévue.
- **Fin de file** : quand tout est terminé, l'événement `queue://finished` ouvre côté interface un compte à rebours de 60 s, puis l'action choisie est exécutée.

**Tech Stack :** Tauri 2, Rust (`download-core`, `rusqlite`, `chrono` pour l'heure locale), React 19, TypeScript, Tailwind, Vitest. Glisser-déposer HTML5 natif, sans nouvelle dépendance.

**Spec :** [`../specs/2026-11-10-flashget-v2-refonte-design.md`](../specs/2026-11-10-flashget-v2-refonte-design.md), §4.6, §4.7, §7.1, §7.2, §8.4, §8.6. Maquette : [`../specs/2026-11-10-flashget-v2-maquettes/04-termines-file-planif.html`](../specs/2026-11-10-flashget-v2-maquettes/04-termines-file-planif.html), blocs « File d'attente » et « Planificateur ».

**Feuille de route :** [`2026-11-10-flashget-v2-feuille-de-route.md`](2026-11-10-flashget-v2-feuille-de-route.md)

## Global Constraints

- Aucun code repris de NeoDLP, aucune mention d'un tiers (spec §2).
- Aucune capacité actuelle ne doit disparaître : priorité (haute, normale, basse), pause, reprise, annulation, Tout suspendre / Tout reprendre de la page Téléchargements, limite de vitesse globale des réglages.
- « Télécharger » démarre dès qu'une place se libère ; les éléments « Ajouter à la file » (`held`) ne démarrent que si l'utilisateur lance la file ou que le planificateur le fait ; **un démarrage manuel passe toujours avant** (spec §7.1, §7.2).
- Le planificateur contrôle toute la file (les `held`) : dans la plage, il les démarre ; hors de la plage, ils ne démarrent pas seuls (spec §7.2).
- « Tout démarrer maintenant » respecte la limite de téléchargements simultanés ; « Suspendre la file » met en pause les téléchargements en cours et empêche les autres de démarrer (spec §8.6).
- **Action de fin de file** : un **seul** réglage, avec les choix ne rien faire, quitter FlashGet, veille, veille prolongée, arrêt. Il est partagé avec les Paramètres de la phase 5, et un compte à rebours de 60 s permet d'annuler (spec §4.7, §8.4).
- Tout texte visible en français via `t()` ; chiffres et heures en JetBrains Mono ; couleurs via les jetons Tailwind existants.
- `cargo` toujours avec `-j 2`, de préférence depuis PowerShell (Git Bash donne des erreurs de métadonnées). Branche `flashget-v2-phase-4`, un commit par tâche.

## Review Focus

1. **Plage qui passe minuit** (22:00 → 06:00) : 23:30 et 03:00 sont dans la plage, 07:00 n'y est pas. Le jour coché est celui du **début** de la plage : une plage du vendredi 22:00 dure jusqu'au samedi 06:00. Test : Task 2 `in_window_overnight`.
2. **Fin de file déclenchée à tort** : l'action ne doit pas se lancer au démarrage de l'application quand la file est déjà vide, ni quand le dernier élément a été annulé ou est en erreur. Elle se lance seulement quand une exécution qui avait démarré se termine avec au moins un téléchargement réussi. Test : Task 3 `end_of_queue_detection`.
3. **Compte à rebours annulé, ou fenêtre cachée** : « Annuler » empêche toute action. Si FlashGet est réduit dans la zone de notification, la fenêtre revient au premier plan pour montrer le compte à rebours. Tests : Task 7 `EndOfQueueDialog`.
4. **Glisser-déposer dans un autre groupe de priorité** : l'élément prend la priorité de son nouveau voisin, et l'ordre affiché correspond toujours à l'ordre réel de démarrage. Test : Task 4 `applyDrop`.
5. **Démarrage manuel pendant une file suspendue** : un clic sur Reprendre d'une ligne démarre l'élément malgré la suspension. Rien d'autre ne démarre tout seul. Test : Task 1 `pick_respects_paused_except_forced`.

---

## Décisions techniques

- **Ordre de démarrage** : priorité (haute, normale, basse), puis `queue_rank` croissant, puis ordre d'arrivée. Un nouvel élément reçoit `max(queue_rank) + 1`. La page File d'attente affiche les éléments `queued` et `held` dans cet ordre exact.
- **Glisser-déposer** : l'élément déposé prend la priorité de l'élément juste au-dessus de lui (ou juste en dessous s'il est déposé en tête). Toute la file est ensuite renumérotée (`reorder_queue(ids)`). Pour le clavier et l'accessibilité, chaque ligne a aussi des boutons « Monter » et « Descendre ».
- **Suspendre la file** : met l'indicateur `queue.paused` (persisté) à vrai et met en pause les téléchargements en cours. **Tout démarrer maintenant** remet l'indicateur à faux, passe tous les `held` en `queued` et relance l'ordonnanceur.
- **Planificateur** : réglages persistés en JSON sous la clé `schedule` de la table `settings` :
  - `enabled` ;
  - `start` et `end` au format `"HH:MM"` ;
  - `days` : 7 booléens, lundi d'abord ;
  - `limit_bps` : `Option<u64>` ;
  - `at_end` : `"pause"` ou `"continue"`.

  Par défaut : désactivé, 02:00 → 07:00, du lundi au vendredi, sans limite, `pause`.
- **Entrée dans la plage** : tous les `held` passent en `queued`, puis on retient leurs id comme « lancés par le planificateur », en mémoire. Si `limit_bps` est défini, la limite globale passe à cette valeur. **Sortie de la plage** : avec `at_end = "pause"`, les éléments lancés par le planificateur qui ne sont pas terminés sont mis en pause et remis en `held`. Dans tous les cas, on revient à la limite des réglages.
- **Action de fin de file** : réglage `end_action` (`none`, `quit`, `sleep`, `hibernate` ou `shutdown`, par défaut `none`), clé `end_action` de la table `settings`. **Détection** :
  - une « exécution » commence quand un téléchargement passe en cours ;
  - quand plus rien n'est `queued`, `held`, en cours ou en enregistrement, que l'exécution avait commencé et qu'au moins un téléchargement de l'exécution est terminé, on émet `queue://finished` (si l'action n'est pas `none`) puis l'exécution est remise à zéro.
- **Commandes Windows** :
  - veille : `rundll32.exe powrprof.dll,SetSuspendState 0,1,0` ;
  - veille prolongée : `shutdown /h` ;
  - arrêt : `shutdown /s /t 0` ;
  - quitter : `app.exit(0)`.
- **Heure locale** : la crate `chrono` (`Local::now()`) côté application ; les fonctions du moteur ne reçoivent que `(jour 0..=6, minute 0..1439)` et restent pures.

## Fichiers

| Fichier | Rôle |
|---|---|
| `download-core/src/model.rs`, `storage.rs`, `scheduler.rs` | `queue_rank`, réordonnancement, ordre de démarrage, file suspendue |
| `download-core/src/planner.rs` (nouveau) | `Schedule`, `EndAction`, `in_window`, `next_start`, `end_action_command`, `RunTracker` |
| `src-tauri/src/queue.rs` (nouveau) | commandes file, planificateur, action de fin, boucle de vérification |
| `src-tauri/src/commands.rs`, `lib.rs`, `Cargo.toml` | branchement, `chrono` |
| `src/api.ts`, `src/types.ts` | types et appels |
| `src/queue/queue.ts`, `src/queue/schedule.ts` (nouveaux) | logique pure de la file et du planificateur |
| `src/pages/QueuePage.tsx`, `src/pages/SchedulerPage.tsx` (nouveaux) | pages |
| `src/components/EndOfQueueDialog.tsx` (nouveau) | compte à rebours |
| `src/navigation.ts`, `src/App.tsx` | entrées du menu, compteurs, intégration |

---

### Task 1 : Rang de file, ordre de démarrage, file suspendue

**Files :** `download-core/src/model.rs`, `storage.rs` (+tests), `scheduler.rs` (+tests), chaque littéral `Download { … }` (≈ 7 sites), `src/types.ts`.

**Interfaces :**
- `Download` gagne `#[serde(default)] pub queue_rank: Option<i64>` ; TS : `queue_rank?: number | null`.
- Stockage :
  - `ALTER TABLE downloads ADD COLUMN queue_rank INTEGER` ;
  - `upsert` pose `queue_rank = COALESCE(stored, incoming, (SELECT COALESCE(MAX(queue_rank), 0) + 1 FROM downloads))` à l'insertion, et ne l'écrase jamais à la mise à jour, sauf par `set_queue_order` ;
  - `pub fn set_queue_order(&self, ids: &[String]) -> Result<(), CoreError>` donne `queue_rank = 1..=n` dans l'ordre reçu, en une transaction ;
  - `pub fn queue_paused(&self) -> Result<bool, CoreError>` et `set_queue_paused(&self, v: bool)` (clé `queue.paused` de la table `settings`, `"1"` ou `"0"`, absente vaut faux).
- `scheduler::pick_to_start(downloads: &[Download], max_concurrent: u32, paused: bool, forced: Option<&str>) -> Vec<String>` :
  - parmi les `Queued`, tri par `(priority_rank, queue_rank.unwrap_or(i64::MAX), position)` ;
  - si `paused`, ne renvoie que `forced` (s'il est `Queued` et qu'une place est libre), sinon rien ;
  - si non suspendu, `forced` passe en premier s'il est `Queued`.
  - Tous les appelants actuels passent `paused` (lu dans le stockage) et `forced: None`, sauf `start_download`, qui passe `Some(&id)`. On change `reevaluate(app)` en `reevaluate_with(app, forced: Option<&str>)`, et `reevaluate(app)` appelle `reevaluate_with(app, None)`.

- [ ] **Step 1 : Tests qui échouent**

```rust
// storage.rs
#[test] fn new_rows_get_increasing_queue_rank() { /* a, b, c insérés → ranks 1, 2, 3 ; re-upsert de b ne change pas son rang */ }
#[test] fn set_queue_order_renumbers() { /* set_queue_order(["c","a","b"]) → c=1, a=2, b=3 */ }
#[test] fn queue_paused_roundtrip() { /* absent → false ; set true → true ; set false → false */ }
// scheduler.rs
#[test] fn rank_breaks_ties_within_priority() { /* deux Normal (rang 5 puis rang 2) et un High (rang 9) → ordre High, rang2, rang5 */ }
#[test] fn pick_respects_paused_except_forced() { /* paused=true, forced None → [] ; forced Some(x Queued) → [x] ; forced sur un Held → [] */ }
#[test] fn forced_goes_first_when_not_paused() { /* max 1, forced = élément Low → [forced] */ }
```

- [ ] **Step 2 : Vérifier l'échec** — `cargo test --manifest-path src-tauri/Cargo.toml -p download-core -j 2`.
- [ ] **Step 3 : Implémenter**, puis adapter `commands.rs` : `reevaluate_with`, et `start_download` qui force son id.
- [ ] **Step 4 : Vérifier** — cargo workspace `-j 2`, `npm test`, `npx tsc --noEmit`.
- [ ] **Step 5 : Commit** — `feat(core): rang de file, ordre de démarrage et file suspendue`

---

### Task 2 : Logique pure du planificateur

**Files :** create `download-core/src/planner.rs` (+tests) ; modify `download-core/src/lib.rs`.

**Interfaces :**
- `#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)] pub struct Schedule { pub enabled: bool, pub start: String, pub end: String, pub days: [bool; 7], pub limit_bps: Option<u64>, pub at_end: AtEnd }`, avec `Default` (désactivé, `"02:00"`, `"07:00"`, `[true,true,true,true,true,false,false]`, `None`, `AtEnd::Pause`).
- `#[serde(rename_all = "snake_case")] pub enum AtEnd { Pause, Continue }`
- `#[serde(rename_all = "snake_case")] pub enum EndAction { None, Quit, Sleep, Hibernate, Shutdown }` (+ `Default = None`).
- `pub fn parse_hhmm(s: &str) -> Option<u16>`, en minutes 0..1439 ; `"24:00"` et `"7:5"` sont invalides.
- `pub fn validate(s: &Schedule) -> Result<(), String>` : heures valides, `start != end`, au moins un jour coché si `enabled`.
- `pub fn in_window(s: &Schedule, weekday: u8 /*0=lundi*/, minute: u16) -> bool` : `false` si la planification est désactivée.
  - Plage dans la journée (`start < end`) : jour coché ET `start ≤ m < end`.
  - Plage qui passe minuit (`start > end`) : (jour coché ET `m ≥ start`) OU (veille cochée ET `m < end`).
- `pub fn next_start(s: &Schedule, weekday: u8, minute: u16) -> Option<(u8 /*jours d'écart 0..=7*/, u16 /*minute*/)>` : prochain début strictement après maintenant ; `None` si la planification est désactivée ou qu'aucun jour n'est coché.
- `pub fn end_action_command(a: EndAction) -> Option<(&'static str, Vec<&'static str>)>` : `Sleep` → `("rundll32.exe", ["powrprof.dll,SetSuspendState", "0,1,0"])`, `Hibernate` → `("shutdown", ["/h"])`, `Shutdown` → `("shutdown", ["/s","/t","0"])`, `None` et `Quit` → `None` (quitter est géré par l'application).
- `#[derive(Default)] pub struct RunTracker { active: bool, completed: u32 }` avec `pub fn on_started(&mut self)`, `pub fn on_completed(&mut self)`, et `pub fn check_finished(&mut self, pending: usize) -> bool`. `pending` est le nombre d'éléments `queued`, `held`, en cours ou en enregistrement. La fonction renvoie `true` une seule fois, quand `active && pending == 0 && completed > 0`, puis remet tout à zéro. Si `pending == 0` sans aucun terminé, elle remet à zéro et renvoie `false`.

- [ ] **Step 1 : Tests qui échouent**

```rust
#[test] fn parse_hhmm_cases() { assert_eq!(parse_hhmm("02:00"), Some(120)); assert_eq!(parse_hhmm("23:59"), Some(1439)); assert_eq!(parse_hhmm("24:00"), None); assert_eq!(parse_hhmm("7:5"), None); }
#[test] fn in_window_same_day() { /* 02:00-07:00 lun-ven : lundi 03:00 → vrai ; lundi 07:00 → faux ; samedi 03:00 → faux ; désactivé → faux */ }
#[test] fn in_window_overnight() { /* 22:00-06:00, vendredi seul coché : ven 23:30 → vrai ; sam 03:00 → vrai ; sam 07:00 → faux ; jeu 23:30 → faux */ }
#[test] fn next_start_cases() { /* lun 01:00 → (0,120) ; lun 03:00 → (1,120) ; ven 03:00 (lun-ven) → (3,120) ; aucun jour → None */ }
#[test] fn validate_cases() { /* start==end → Err ; heure invalide → Err ; enabled sans jour → Err */ }
#[test] fn end_action_commands() { /* Sleep → rundll32 ; Hibernate → shutdown /h ; Shutdown → shutdown /s /t 0 ; None et Quit → None */ }
#[test] fn end_of_queue_detection() {
    let mut r = RunTracker::default();
    assert!(!r.check_finished(0));                 // démarrage de l'app, file vide : rien
    r.on_started(); assert!(!r.check_finished(1));
    r.on_completed(); assert!(r.check_finished(0)); // fin d'une exécution réussie
    assert!(!r.check_finished(0));                 // une seule fois
    r.on_started(); assert!(!r.check_finished(0)); // exécution annulée ou en erreur, sans succès
}
#[test] fn schedule_json_roundtrip() { /* serde_json : at_end "pause", days tableau de 7 */ }
```

- [ ] **Step 2 à 4 :** échec, implémentation, puis `cargo test … -p download-core -j 2`.
- [ ] **Step 5 : Commit** — `feat(core): logique du planificateur et de la fin de file`

---

### Task 3 : Commandes de file, boucle du planificateur, action de fin

**Files :** create `src-tauri/src/queue.rs` (+tests de la partie pure) ; modify `src-tauri/src/commands.rs`, `src-tauri/src/state.rs`, `src-tauri/src/lib.rs`, `src-tauri/Cargo.toml` (`chrono = { version = "0.4", default-features = false, features = ["clock"] }`).

**Interfaces :**
- Commandes :
  - `reorder_queue(ids: Vec<String>)` → `set_queue_order` puis `reevaluate` ;
  - `start_queue()` → `set_queue_paused(false)`, passage de tous les `held` en `queued`, puis `reevaluate` ;
  - `pause_queue()` → `set_queue_paused(true)`, puis met en pause les téléchargements en cours (comme `pause_all`) ;
  - `get_queue_paused() -> bool` ;
  - `get_schedule() -> Schedule` et `set_schedule(s: Schedule)`, qui appelle `validate`, enregistre le JSON sous la clé `schedule` et déclenche une vérification immédiate ;
  - `get_end_action() -> EndAction` et `set_end_action(a: EndAction)` ;
  - `run_end_action()` : `Quit` → `app.exit(0)` ; sinon lance la commande d'`end_action_command` avec `std::process::Command` (`CREATE_NO_WINDOW` sous Windows) ; `None` → rien.
- `AppState` gagne `planner: Mutex<PlannerState>`, avec `PlannerState { in_window: bool, launched: HashSet<String>, run: RunTracker }`.
- **Boucle** lancée dans `setup` : `tokio` toutes les 30 s, plus un premier passage immédiat, appelle `planner_tick(&app)`.
  - Lit `Schedule` et l'heure locale (`chrono::Local::now()` : `weekday().num_days_from_monday()`, `hour*60+minute`).
  - **Transition hors plage → dans la plage** : chaque `held` passe en `queued` et son id est ajouté à `launched`. Si `limit_bps` est défini, `limiter.set_rate(limit)`. Puis `set_queue_paused(false)` et `reevaluate`.
  - **Transition dans la plage → hors plage** : avec `at_end == Pause`, chaque id de `launched` non terminé est mis en pause puis passé en `held`. On rétablit `limiter.set_rate(settings.max_speed_bps)` et on vide `launched`.
  - La transition se calcule par une fonction **pure** `pub fn planner_transition(was_in: bool, now_in: bool) -> Transition { None, Enter, Exit }`, testée.
- **Suivi de la fin de file** : à chaque passage d'un téléchargement en cours (`mark_active` dans `reevaluate`), `run.on_started()`. À chaque passage en terminé (relais de progression), `run.on_completed()`. Après chaque statut terminal persisté, on compte les éléments en attente ; si `run.check_finished(pending)` renvoie vrai et que `end_action != None`, on émet `queue://finished` avec `{ action: EndAction }`, et la fenêtre principale passe au premier plan (`focus_main_window`).

- [ ] **Step 1 : Tests qui échouent** (dans `queue.rs`, partie pure) : `planner_transition` sur les 4 combinaisons ; `pending_count(&[Download])` compte `queued`, `held`, `downloading` et `recording`, mais pas `paused`, `error` ni `completed`.
- [ ] **Step 2 à 4 :** échec, implémentation, puis cargo workspace `-j 2` et `cargo build -j 2`.
- [ ] **Step 5 : Commit** — `feat: commandes de file, boucle du planificateur et action de fin de file`

---

### Task 4 : API front et logique pure (file, planificateur)

**Files :** modify `src/api.ts` ; create `src/queue/queue.ts`, `src/queue/schedule.ts`, `src/queue/queue.test.ts`.

**Interfaces :**
- `api.ts` :
  - `type AtEnd = "pause" | "continue"` ;
  - `type EndAction = "none" | "quit" | "sleep" | "hibernate" | "shutdown"` ;
  - `interface Schedule { enabled: boolean; start: string; end: string; days: boolean[]; limit_bps: number | null; at_end: AtEnd }` ;
  - `reorderQueue(ids)`, `startQueue()`, `pauseQueue()`, `getQueuePaused()`, `getSchedule()`, `setSchedule(s)` (→ `{ s }`), `getEndAction()`, `setEndAction(a)` (→ `{ a }`), `runEndAction()` ;
  - `onQueueFinished(cb: (e: { action: EndAction }) => void)`.
- `queue.ts` :
  - `queueItems(list: Download[]): Download[]` : `queued` et `held`, triés exactement comme le Rust (priorité, `queue_rank` avec `null` en dernier, position) ;
  - `applyDrop(items: Download[], from: number, to: number): { order: string[]; moved: string; priority: Priority }` : la nouvelle priorité est celle de l'élément juste au-dessus après le déplacement, ou juste en dessous si l'élément arrive en tête ;
  - `queueSummary(items): { count: number; bytes: number | null }` : `bytes` est `null` si une taille manque.
- `schedule.ts` :
  - `parseHHMM(s): number | null`, `inWindow(s, weekday, minute)` et `nextStart(s, weekday, minute)` : mêmes règles que le Rust ;
  - `timelineSegments(s): { left: number; width: number }[]` : en pourcentages de 24 h, deux segments si la plage passe minuit ;
  - `nextStartLabel(s, now: Date): string | null` : `t("sch.tonight", { time })` si c'est le même soir ou la nuit qui suit (moins de 24 h et avant 06:00 le lendemain), `t("sch.tomorrow", { time })`, ou `t("sch.onDay", { day, time })` avec le nom du jour en français.
- Textes :

| Clé | Texte |
|---|---|
| `sch.tonight` | cette nuit à {time} |
| `sch.tomorrow` | demain à {time} |
| `sch.onDay` | {day} à {time} |

- [ ] **Step 1 : Tests qui échouent** :
  - `queueItems` : l'ordre (High avant Normal, puis le rang, `null` en dernier) et l'exclusion des terminés, des éléments en cours et en pause ;
  - `applyDrop` : un déplacement à l'intérieur d'un groupe garde la priorité ; dépasser un élément High donne la priorité High ; une arrivée en tête prend la priorité de l'élément suivant ;
  - `timelineSegments` : 02:00-07:00 donne un segment (8,33 % ; 20,83 %), 22:00-06:00 en donne deux ;
  - `inWindow` et `nextStart` : mêmes cas que la Task 2 ;
  - `nextStartLabel` : un lundi 23:00 avec début à 02:00 donne « cette nuit à 02:00 ».
- [ ] **Step 2 à 4 :** échec, implémentation, puis `npm test` et `tsc`.
- [ ] **Step 5 : Commit** — `feat(ui): API et logique de la file et du planificateur`

---

### Task 5 : Page File d'attente

**Files :** create `src/pages/QueuePage.tsx`, `src/pages/QueuePage.test.tsx` ; modify `src/i18n/fr.ts`.

**Interfaces :**
- `QueuePage({ downloads: Download[]; maxConcurrent: number; schedule: Schedule | null; paused: boolean; onChanged(): void })` (maquette 04, bloc « File d'attente »).
- **En-tête** :
  - titre ;
  - résumé `t("q.summary", { n, size, max })` (« 4 en attente · 2,4 Go · démarrent dès qu'une place se libère (3 simultanés max) »), avec `—` à la place de la taille si une taille manque ;
  - si `paused`, une étiquette `t("q.pausedBadge")` ;
  - boutons « Suspendre la file » (`api.pauseQueue`) et « Tout démarrer maintenant » (`cta`, `api.startQueue`).
- **Lignes** : poignée (`GripVertical`), rang (1..n), vignette (`Thumb`), nom et site, puis :
  - si l'élément est `held` et le planificateur actif, `t("q.scheduled", { time: schedule.start })` en `accent-fg` ;
  - si l'élément est `held` sans planificateur, `t("dl.held")` ;
  - sélecteur de priorité (Haute, Normale, Basse → `api.setPriority`), taille, boutons Monter et Descendre, et Retirer (`api.removeFromList` après une confirmation `window.confirm(t("q.removeConfirm"))`).
- **Glisser-déposer HTML5** : `draggable` sur la ligne ; `onDragOver` affiche une ligne de dépôt `accent` ; `onDrop` utilise `applyDrop`, puis `api.setPriority(moved, priority)` si la priorité change, puis `api.reorderQueue(order)`, puis `onChanged()`. Monter et Descendre passent par le même chemin.
- **File vide** : `t("q.empty")`.
- Textes :

| Clé | Texte |
|---|---|
| `q.title` | File d'attente |
| `q.summary` | {n} en attente · {size} · démarrent dès qu'une place se libère ({max} simultanés max) |
| `q.pause` | Suspendre la file |
| `q.startAll` | Tout démarrer maintenant |
| `q.pausedBadge` | File suspendue |
| `q.scheduled` | programmé à {time} |
| `q.up` | Monter |
| `q.down` | Descendre |
| `q.remove` | Retirer |
| `q.removeConfirm` | Retirer cet élément de la file ? |
| `q.empty` | La file est vide. |

- [ ] **Step 1 : Tests qui échouent** :
  - lignes numérotées dans l'ordre de `queueItems` ;
  - le résumé ;
  - « Tout démarrer » appelle `startQueue`, puis `onChanged` ;
  - « Suspendre » appelle `pauseQueue` ;
  - glisser la ligne 3 sur la ligne 1 appelle `reorderQueue` avec le nouvel ordre, et `setPriority` si nécessaire (`fireEvent.dragStart`, `dragOver`, `drop`) ;
  - Monter et Descendre ;
  - « programmé à 02:00 » sur un `held` quand le planificateur est actif ;
  - Retirer avec confirmation (`window.confirm` simulé).
- [ ] **Step 2 à 4 :** échec, implémentation, puis `npm test` et `tsc`.
- [ ] **Step 5 : Commit** — `feat(ui): page File d'attente (ordre, priorité, glisser-déposer)`

---

### Task 6 : Page Planificateur

**Files :** create `src/pages/SchedulerPage.tsx`, `src/pages/SchedulerPage.test.tsx` ; modify `src/i18n/fr.ts`.

**Interfaces :**
- `SchedulerPage({ heldCount: number })` (maquette 04, bloc « Planificateur »).
- Au montage, lit `api.getSchedule()` et `api.getEndAction()`.
- **En-tête** :
  - titre ;
  - `t("sch.next", { when: nextStartLabel(...), n: heldCount })` si la planification est active, sinon `t("sch.off")` ;
  - interrupteur Activé.
- **Carte « Quand »** :
  - heures de début et de fin (`<input type="time">`) ;
  - 7 boutons de jours L, M, M, J, V, S, D (`aria-pressed`) ;
  - frise 24 h (`timelineSegments`) avec les repères 00h, 06h, 12h, 18h, 24h.
- **Carte « Pendant la plage »** :
  - interrupteur et champ « Limiter la vitesse » en Mo/s (convertis en octets/s) ;
  - sélecteur « À {end}, si ce n'est pas fini » : Mettre en pause ou Continuer.
- **Carte « Quand tout est terminé »** : sélecteur d'action (Ne rien faire, Quitter FlashGet, Mettre le PC en veille, Mettre en veille prolongée, Éteindre le PC), avec la note `t("sch.countdownNote")`.
- **Enregistrement** : chaque modification appelle `api.setSchedule` (ou `setEndAction`). Une erreur de validation renvoyée par le Rust s'affiche sous la carte concernée, et l'état affiché reste celui que l'utilisateur a saisi.
- Textes :

| Clé | Texte |
|---|---|
| `sch.title` | Planificateur |
| `sch.enabled` | Activé |
| `sch.next` | Prochain démarrage : {when} · {n} téléchargements concernés |
| `sch.off` | Planification désactivée |
| `sch.when` | Quand |
| `sch.startAt` | Démarrer la file à |
| `sch.until` | jusqu'à |
| `sch.days` | Jours |
| `sch.during` | Pendant la plage |
| `sch.limit` | Limiter la vitesse |
| `sch.limitHint` | Pour garder de la bande passante pour le reste de la maison. |
| `sch.atEnd` | À {end}, si ce n'est pas fini |
| `sch.atEndPause` | Mettre en pause |
| `sch.atEndContinue` | Continuer |
| `sch.done` | Quand tout est terminé |
| `sch.action` | Action |
| `sch.countdownNote` | Un compte à rebours de 60 s permet d'annuler. |
| `end.none` | Ne rien faire |
| `end.quit` | Quitter FlashGet |
| `end.sleep` | Mettre le PC en veille |
| `end.hibernate` | Mettre en veille prolongée |
| `end.shutdown` | Éteindre le PC |

  Noms des jours : `day.0` à `day.6` (« lundi » … « dimanche ») et initiales `dayShort.0` à `dayShort.6` (« L », « M », « M », « J », « V », « S », « D »).

- [ ] **Step 1 : Tests qui échouent** :
  - chargement : valeurs affichées et frise ;
  - activer la planification appelle `setSchedule` avec `enabled: true` ;
  - cocher samedi ;
  - la limite de 10 Mo/s donne `limit_bps = 10485760` ;
  - le sélecteur d'action appelle `setEndAction("sleep")` ;
  - une erreur de validation renvoyée (début égal à la fin) est affichée ;
  - « Prochain démarrage » s'affiche avec le nombre d'éléments en attente.
- [ ] **Step 2 à 4 :** échec, implémentation, puis `npm test` et `tsc`.
- [ ] **Step 5 : Commit** — `feat(ui): page Planificateur`

---

### Task 7 : Compte à rebours de fin de file et intégration

**Files :** create `src/components/EndOfQueueDialog.tsx`, `src/components/EndOfQueueDialog.test.tsx` ; modify `src/navigation.ts` (`queue` et `scheduler` en `ready: true`), `src/App.tsx`, `src/i18n/fr.ts`.

**Interfaces :**
- `EndOfQueueDialog({ action: EndAction; onCancel(): void; onRun(): void })` : fenêtre modale qui affiche `t("eoq.title")`, puis `t("eoq.body", { action: t("end.<action>"), s })`, avec `s` qui décompte de 60 à 0 chaque seconde. À 0, `onRun()` est appelé **une seule fois**. Deux boutons : « Annuler » (focus initial, Échap) et « Maintenant » (`onRun`).
- `App.tsx` :
  - les pages `queue` et `scheduler` sont rendues ;
  - compteurs du menu : `queue` = nombre de `queued` + `held` ;
  - `QueuePage` reçoit `maxConcurrent` (lu avec `getSettings`), le `schedule` (`getSchedule`, relu au retour depuis la page Planificateur) et `paused` (`getQueuePaused`, relu après chaque action de file) ;
  - `onQueueFinished` affiche `EndOfQueueDialog` ; `onRun` appelle `api.runEndAction()` puis ferme la fenêtre ; « Annuler » la ferme sans rien faire.
- Textes :

| Clé | Texte |
|---|---|
| `eoq.title` | Tous les téléchargements sont terminés |
| `eoq.body` | {action} dans {s} s. |
| `eoq.cancel` | Annuler |
| `eoq.now` | Maintenant |

- [ ] **Step 1 : Tests qui échouent** (minuteries simulées) :
  - le décompte va de 60 à 59 ;
  - à 60 s, `onRun` est appelé une seule fois ;
  - « Annuler » puis 60 s : `onRun` n'est jamais appelé ;
  - « Maintenant » appelle `onRun` ;
  - Échap annule ;
  - les entrées « File d'attente » et « Planificateur » sont visibles dans le menu, dans l'ordre du spec (adapter `shell.test.tsx`).
- [ ] **Step 2 : Vérifier l'échec.**
- [ ] **Step 3 : Implémenter.**
- [ ] **Step 4 : Vérifier** — `npm test`, `npx tsc --noEmit`, `npm run build`, cargo workspace `-j 2`, `cargo build -j 2`.
- [ ] **Step 5 : Commit** — `feat(ui): file et planificateur branchés, compte à rebours de fin de file`
- [ ] **Step 6 : Vérification manuelle** (par le propriétaire, `npm run tauri dev`, après avoir fermé tout FlashGet et tout Vite sur le port 1420) :
  1. Ajouter 3 fichiers avec « Ajouter à la file » : ils apparaissent numérotés dans File d'attente. Les réordonner en les glissant ; passer l'un en priorité haute. « Tout démarrer maintenant » les lance dans l'ordre affiché, avec la limite de simultanés respectée.
  2. « Suspendre la file » : les téléchargements en cours se mettent en pause et rien ne démarre seul ; « Reprendre » sur une ligne démarre cet élément seul.
  3. Planificateur : activer une plage qui commence dans 2 minutes, avec une limite de 1 Mo/s. Les éléments en attente démarrent à l'heure, limités. À la fin de la plage, ils repassent en attente.
  4. Action « Quitter FlashGet » : à la fin du dernier téléchargement, le compte à rebours s'affiche et « Annuler » empêche de quitter. Recommencer sans annuler : FlashGet se ferme au bout de 60 s.
- [ ] **Step 7 : Fin de phase** — `superpowers:finishing-a-development-branch`.
