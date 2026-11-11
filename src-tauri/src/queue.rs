//! File d'attente et planificateur : commandes, boucle de vérification, action de fin.
//!
//! Ordre des verrous : `handles` → `MARKS_IO` → `planner`. Le verrou `planner` est toujours une
//! feuille : aucune autre ressource (stockage, `handles`, émission, `reevaluate`) n'est prise tant
//! qu'il est tenu.

use crate::commands::{focus_main_window, reevaluate};
use crate::state::AppState;
use download_core::model::{Download, DownloadStatus};
use download_core::planner::{self, AtEnd, EndAction, RunTracker, Schedule};
use download_core::storage::Storage;
use std::collections::HashSet;
use std::sync::{Mutex, MutexGuard};
use tauri::{AppHandle, Emitter, Manager, State};

const SCHEDULE_KEY: &str = "schedule";
const END_ACTION_KEY: &str = "end_action";
/// Marques du planificateur persistées (JSON `PlannerMarks`).
const MARKS_KEY: &str = "planner.marks";
/// Période de la boucle du planificateur.
const TICK_SECS: u64 = 30;

/// État en mémoire du planificateur et du suivi de fin de file.
#[derive(Default)]
pub struct PlannerState {
    /// Dernier état connu « dans la plage horaire ».
    pub in_window: bool,
    /// Ids passés de `held` à `queued` par le planificateur pendant la plage (persisté).
    pub launched: HashSet<String>,
    /// Ids mis en pause par « Suspendre la file » (persisté) : « Tout démarrer maintenant » et
    /// l'entrée dans la plage les remettent en file ; une pause individuelle reste en pause.
    pub suspend_paused: HashSet<String>,
    /// Exécution en cours (détection de la fin de file).
    pub run: RunTracker,
    /// Ids mis en pause à la sortie de la plage : leur `Paused` est persisté en `Held`.
    pub hold_on_pause: HashSet<String>,
}

/// Verrou du planificateur, tolérant à l'empoisonnement (un panic ailleurs ne doit pas
/// arrêter la boucle de 30 s : l'état reste cohérent, chaque section critique est courte).
fn planner_lock(st: &AppState) -> MutexGuard<'_, PlannerState> {
    st.planner.lock().unwrap_or_else(|e| e.into_inner())
}

/// Marques du planificateur qui doivent survivre à un redémarrage (clé `planner.marks`).
#[derive(serde::Serialize, serde::Deserialize, Default, Debug, PartialEq)]
#[serde(default)]
struct PlannerMarks {
    launched: Vec<String>,
    suspend_paused: Vec<String>,
}

fn sorted(set: &HashSet<String>) -> Vec<String> {
    let mut v: Vec<String> = set.iter().cloned().collect();
    v.sort();
    v
}

fn load_marks(storage: &Storage) -> PlannerMarks {
    storage
        .get_pref(MARKS_KEY)
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn write_marks(storage: &Storage, m: &PlannerMarks) -> Result<(), String> {
    let json = serde_json::to_string(m).map_err(|e| e.to_string())?;
    storage.set_pref(MARKS_KEY, &json).map_err(|e| e.to_string())
}

/// Sérialise les écritures de `planner.marks` : l'instantané est pris PUIS écrit sous ce verrou,
/// donc la dernière écriture reflète toujours l'état le plus récent. Le verrou `planner` est
/// relâché avant l'écriture (il reste une feuille).
static MARKS_IO: Mutex<()> = Mutex::new(());

/// À appeler après toute modification de `launched` / `suspend_paused`.
fn persist_marks(st: &AppState) {
    let _io = MARKS_IO.lock().unwrap_or_else(|e| e.into_inner());
    let marks = {
        let p = planner_lock(st);
        PlannerMarks { launched: sorted(&p.launched), suspend_paused: sorted(&p.suspend_paused) }
    };
    if let Err(e) = write_marks(&st.storage, &marks) {
        eprintln!("planificateur : enregistrement des marques impossible : {e}");
    }
}

/// Au démarrage : recharge les marques persistées dans l'état en mémoire.
pub(crate) fn load_planner_marks(st: &AppState) {
    let m = load_marks(&st.storage);
    let mut p = planner_lock(st);
    p.launched.extend(m.launched);
    p.suspend_paused.extend(m.suspend_paused);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transition {
    None,
    Enter,
    Exit,
}

pub fn planner_transition(was_in: bool, now_in: bool) -> Transition {
    match (was_in, now_in) {
        (false, true) => Transition::Enter,
        (true, false) => Transition::Exit,
        _ => Transition::None,
    }
}

/// Pendant une plage active (sans transition), les `held` ajoutés entre-temps sont lâchés
/// dans la file à chaque passage — sauf si la file a été suspendue dans la plage.
pub fn release_in_window(t: Transition, now_in: bool, queue_paused: bool) -> bool {
    t == Transition::None && now_in && !queue_paused
}

/// Hors plage alors que des éléments lancés par le planificateur sont encore marqués (sortie
/// manquée, ex. application fermée pendant la plage puis relancée après) : appliquer la sortie.
/// Une sortie normale vide `launched`, donc ceci ne se produit qu'une fois.
pub fn startup_exit_needed(in_window: bool, launched_nonempty: bool) -> bool {
    !in_window && launched_nonempty
}

/// Ce que la sortie de plage (`at_end = pause`) fait d'un élément lancé par le planificateur.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitAction {
    /// Pas démarré : repasse directement en `held`.
    Hold,
    /// Actif : mise en pause, puis le relais persiste `held` au lieu de `paused`.
    PauseThenHold,
    /// Laisser tel quel : mis en pause par l'utilisateur (ou « Suspendre la file »), déjà
    /// `held`, terminé, en erreur, ou enregistrement live (laissé en cours, comme `pause_all`
    /// n'arrête pas non plus un live : seul « Arrêter » le finalise).
    Leave,
}

pub fn exit_action(status: DownloadStatus, has_handle: bool) -> ExitAction {
    match (status, has_handle) {
        (DownloadStatus::Queued, false) => ExitAction::Hold,
        (DownloadStatus::Queued | DownloadStatus::Downloading, true) => ExitAction::PauseThenHold,
        _ => ExitAction::Leave,
    }
}

/// Éléments encore « à faire » pour la fin de file : en attente (`queued`, `held`), en cours
/// (`downloading`, `recording`) ou en pause (`paused` : la file n'est pas finie).
pub fn pending_count(list: &[Download]) -> usize {
    list.iter()
        .filter(|d| {
            matches!(
                d.status,
                DownloadStatus::Queued
                    | DownloadStatus::Held
                    | DownloadStatus::Downloading
                    | DownloadStatus::Recording
                    | DownloadStatus::Paused
            )
        })
        .count()
}

/// Juste avant l'action de fin (après le compte à rebours) : n'agir que si plus rien n'est à faire.
pub fn end_action_still_due(list: &[Download]) -> bool {
    pending_count(list) == 0
}

fn load_schedule(storage: &Storage) -> Schedule {
    storage
        .get_pref(SCHEDULE_KEY)
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn load_end_action(storage: &Storage) -> EndAction {
    storage
        .get_pref(END_ACTION_KEY)
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_value(serde_json::Value::String(s)).ok())
        .unwrap_or_default()
}

fn end_action_str(a: EndAction) -> String {
    serde_json::to_value(a).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_else(|| "none".into())
}

/// Met en pause tous les téléchargements actifs (le moteur persiste ensuite `Paused`).
pub(crate) fn pause_active(state: &AppState) {
    let handles = state.handles.lock().unwrap();
    for h in handles.values() {
        h.pause();
    }
}

/// Passe tous les `held` en `queued` ; renvoie leurs ids.
fn release_held(storage: &Storage) -> Result<Vec<String>, String> {
    let downloads = storage.list().map_err(|e| e.to_string())?;
    let mut ids = Vec::new();
    for d in downloads.iter().filter(|d| d.status == DownloadStatus::Held) {
        storage.update_state(&d.id, d.downloaded_bytes, DownloadStatus::Queued).map_err(|e| e.to_string())?;
        ids.push(d.id.clone());
    }
    Ok(ids)
}

/// Lâche les `held` dans la file au nom du planificateur (retenus dans `launched`).
/// Renvoie le nombre d'éléments lâchés.
fn admit_held(st: &AppState) -> usize {
    let ids = match release_held(&st.storage) {
        Ok(ids) => ids,
        Err(e) => {
            eprintln!("planificateur : passage en file impossible : {e}");
            return 0;
        }
    };
    let n = ids.len();
    if n > 0 {
        planner_lock(st).launched.extend(ids);
        persist_marks(st);
    }
    n
}

/// Repasse en `queued` ceux des `ids` qui sont `paused` (les autres sont laissés tels quels).
fn requeue_paused(storage: &Storage, ids: &[String]) -> Result<(), String> {
    for id in ids {
        if let Some(d) = storage.get_by_id(id).map_err(|e| e.to_string())? {
            if d.status == DownloadStatus::Paused {
                storage.update_state(id, d.downloaded_bytes, DownloadStatus::Queued).map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(())
}

/// Remet en file les éléments mis en pause par « Suspendre la file », puis oublie la marque.
fn requeue_after_suspend(st: &AppState) -> Result<(), String> {
    let ids = sorted(&planner_lock(st).suspend_paused);
    if ids.is_empty() {
        return Ok(());
    }
    requeue_paused(&st.storage, &ids)?;
    forget_suspend_paused(st, &ids);
    Ok(())
}

/// Retire `ids` des éléments « mis en pause par la suspension » (ex. repris à la main : une
/// pause individuelle ultérieure ne doit plus être levée par « Tout démarrer maintenant »).
pub(crate) fn forget_suspend_paused(st: &AppState, ids: &[String]) {
    let changed = {
        let mut p = planner_lock(st);
        let before = p.suspend_paused.len();
        for id in ids {
            p.suspend_paused.remove(id);
        }
        p.suspend_paused.len() != before
    };
    if changed {
        persist_marks(st);
    }
}

/// Débit global : limite de la plage si dans la plage (et qu'elle en a une), sinon réglages.
/// `set_rate` est idempotent : appelé à chaque passage et à chaque enregistrement des réglages.
fn set_effective_rate(st: &AppState, schedule: &Schedule, in_window: bool) {
    let max = st.storage.get_settings().map(|s| s.max_speed_bps).unwrap_or(0);
    st.limiter.set_rate(planner::effective_rate(max, schedule, in_window));
}

/// Applique le débit effectif selon le dernier état « dans la plage » connu du planificateur
/// (appelé après un enregistrement des réglages).
pub(crate) fn apply_effective_rate(st: &AppState) {
    let in_window = planner_lock(st).in_window;
    set_effective_rate(st, &load_schedule(&st.storage), in_window);
}

// --- Hooks appelés depuis commands.rs / library.rs ---

/// Un téléchargement vient de passer en cours (`mark_active` dans `reevaluate`).
pub(crate) fn on_started(app: &AppHandle) {
    planner_lock(&app.state::<AppState>()).run.on_started();
}

/// Retire `id` de la liste « à remettre en attente » ; `true` s'il y figurait.
pub(crate) fn take_hold_on_pause(app: &AppHandle, id: &str) -> bool {
    planner_lock(&app.state::<AppState>()).hold_on_pause.remove(id)
}

/// Après un retrait de la liste (annuler, retirer, supprimer) : si plus rien n'est en
/// attente, l'exécution est abandonnée SANS émettre (un ancien succès ne doit pas faire
/// déclencher l'action de fin par une exécution ultérieure qui échoue).
pub(crate) fn reset_run_if_idle(st: &AppState) {
    let pending = match st.storage.list() {
        Ok(list) => pending_count(&list),
        Err(_) => return,
    };
    if pending == 0 {
        planner_lock(st).run.reset();
    }
}

/// À appeler APRÈS la persistance d'un statut terminal. `first` = première persistance de ce
/// statut pour ce téléchargement (évite de compter deux fois un même `Completed`).
/// Sur `Completed`/`Error`, émet `queue://finished` si l'exécution s'est vidée avec au moins un
/// succès et qu'une action de fin est choisie. `Paused`/`Held` (suspension voulue par
/// l'utilisateur ou le planificateur) ne déclenchent pas la fin de file.
pub(crate) fn on_terminal_persisted(app: &AppHandle, status: DownloadStatus, first: bool) {
    let st = app.state::<AppState>();
    if status == DownloadStatus::Completed && first {
        planner_lock(&st).run.on_completed();
    }
    if !matches!(status, DownloadStatus::Completed | DownloadStatus::Error) {
        return;
    }
    let pending = match st.storage.list() {
        Ok(list) => pending_count(&list),
        Err(e) => {
            eprintln!("fin de file : lecture impossible : {e}");
            return;
        }
    };
    let finished = planner_lock(&st).run.check_finished(pending);
    if !finished {
        return;
    }
    let action = load_end_action(&st.storage);
    if action == EndAction::None {
        return;
    }
    let _ = app.emit("queue://finished", serde_json::json!({ "action": action }));
    focus_main_window(app);
}

// --- Planificateur ---

/// Un passage du planificateur à l'heure locale courante. Aussi appelé tout de suite après
/// un ajout « sans démarrer » (pour qu'un `held` ajouté dans la plage parte sans attendre 30 s).
pub(crate) fn planner_tick(app: &AppHandle) {
    use chrono::{Datelike, Timelike};
    let now = chrono::Local::now();
    let weekday = now.weekday().num_days_from_monday() as u8;
    let minute = (now.hour() * 60 + now.minute()) as u16;
    planner_tick_at(app, weekday, minute);
}

fn planner_tick_at(app: &AppHandle, weekday: u8, minute: u16) {
    let st = app.state::<AppState>();
    let schedule = load_schedule(&st.storage);
    let now_in = planner::in_window(&schedule, weekday, minute);
    // Transition calculée et appliquée à l'état sous un seul verrou court : deux passages
    // concurrents (boucle + set_schedule) ne peuvent pas agir deux fois.
    let (transition, exiting) = {
        let mut p = planner_lock(&st);
        let t = planner_transition(p.in_window, now_in);
        p.in_window = now_in;
        let exit = t == Transition::Exit || startup_exit_needed(now_in, !p.launched.is_empty());
        let launched = if exit { Some(std::mem::take(&mut p.launched)) } else { None };
        (t, launched)
    };
    // Débit appliqué à chaque passage (avant tout démarrage) : limite de plage ou réglages.
    set_effective_rate(&st, &schedule, now_in);
    if let Some(launched) = exiting {
        persist_marks(&st);
        exit_window(app, &schedule, launched);
    } else if transition == Transition::Enter {
        enter_window(app);
    } else if release_in_window(transition, now_in, st.storage.queue_paused().unwrap_or(false))
        && admit_held(&st) > 0
    {
        reevaluate(app);
    }
}

fn enter_window(app: &AppHandle) {
    let st = app.state::<AppState>();
    admit_held(&st);
    if let Err(e) = st.storage.set_queue_paused(false) {
        eprintln!("planificateur : reprise de la file impossible : {e}");
    }
    if let Err(e) = requeue_after_suspend(&st) {
        eprintln!("planificateur : reprise des éléments suspendus impossible : {e}");
    }
    reevaluate(app);
}

fn exit_window(app: &AppHandle, schedule: &Schedule, launched: HashSet<String>) {
    let st = app.state::<AppState>();
    if schedule.at_end == AtEnd::Pause {
        for id in launched {
            // `handles` tenu pendant lecture + décision : un démarrage concurrent ne peut pas
            // s'intercaler (et `mark_active` ignore désormais les `held`).
            let handles = st.handles.lock().unwrap();
            let d = match st.storage.get_by_id(&id) {
                Ok(Some(d)) => d,
                _ => continue,
            };
            let handle = handles.get(&id);
            match exit_action(d.status, handle.is_some()) {
                ExitAction::Hold => {
                    if let Err(e) = st.storage.update_state(&id, d.downloaded_bytes, DownloadStatus::Held) {
                        eprintln!("planificateur : mise en attente de {id} impossible : {e}");
                    }
                }
                ExitAction::PauseThenHold => {
                    if let Some(h) = handle {
                        planner_lock(&st).hold_on_pause.insert(id.clone());
                        h.pause();
                    }
                }
                ExitAction::Leave => {}
            }
        }
    }
}

/// Boucle du planificateur : recharge les marques persistées, puis un passage immédiat et
/// toutes les 30 s (le premier passage applique une sortie de plage manquée).
/// Le premier passage est synchrone, avant de rendre la main : l'appelant peut relancer la
/// file ensuite sans que des téléchargements récupérés démarrent hors de la plage horaire.
pub(crate) fn start_planner_loop(app: AppHandle) {
    load_planner_marks(&app.state::<AppState>());
    planner_tick(&app);
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(TICK_SECS));
        // Le premier `tick` d'un intervalle est immédiat : on le consomme, le passage initial
        // a déjà eu lieu. Aucun verrou n'est tenu à travers l'`await`.
        interval.tick().await;
        loop {
            interval.tick().await;
            planner_tick(&app);
        }
    });
}

// --- Commandes ---

#[tauri::command]
pub fn reorder_queue(app: AppHandle, ids: Vec<String>) -> Result<(), String> {
    app.state::<AppState>().storage.set_queue_order(&ids).map_err(|e| e.to_string())?;
    reevaluate(&app);
    Ok(())
}

/// « Tout démarrer maintenant », sans la ré-évaluation : lève la suspension, passe les `held`
/// en `queued` et remet en file les éléments mis en pause par « Suspendre la file ».
fn resume_queue(st: &AppState) -> Result<(), String> {
    st.storage.set_queue_paused(false).map_err(|e| e.to_string())?;
    release_held(&st.storage)?;
    requeue_after_suspend(st)
}

#[tauri::command]
pub fn start_queue(app: AppHandle) -> Result<(), String> {
    resume_queue(&app.state::<AppState>())?;
    reevaluate(&app);
    Ok(())
}

/// « Suspendre la file » : pose l'indicateur, remet en attente (`held`) les `queued` qui
/// attendent, met en pause les téléchargements actifs et retient leurs ids (persistés).
fn suspend_queue(st: &AppState) -> Result<(), String> {
    let running: Vec<String> = {
        // `handles` tenu : aucun démarrage ne s'intercale entre la mise en attente et la pause.
        let handles = st.handles.lock().unwrap();
        st.storage.set_queue_paused(true).map_err(|e| e.to_string())?;
        let downloads = st.storage.list().map_err(|e| e.to_string())?;
        for d in downloads.iter().filter(|d| d.status == DownloadStatus::Queued && !handles.contains_key(&d.id)) {
            st.storage.update_state(&d.id, d.downloaded_bytes, DownloadStatus::Held).map_err(|e| e.to_string())?;
        }
        for h in handles.values() {
            h.pause();
        }
        handles.keys().cloned().collect()
    };
    if !running.is_empty() {
        planner_lock(st).suspend_paused.extend(running);
        persist_marks(st);
    }
    Ok(())
}

#[tauri::command]
pub fn pause_queue(state: State<'_, AppState>) -> Result<(), String> {
    suspend_queue(&state)
}

#[tauri::command]
pub fn get_queue_paused(state: State<'_, AppState>) -> Result<bool, String> {
    state.storage.queue_paused().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_schedule(state: State<'_, AppState>) -> Schedule {
    load_schedule(&state.storage)
}

#[tauri::command]
pub fn set_schedule(app: AppHandle, s: Schedule) -> Result<(), String> {
    planner::validate(&s)?;
    let json = serde_json::to_string(&s).map_err(|e| e.to_string())?;
    app.state::<AppState>().storage.set_pref(SCHEDULE_KEY, &json).map_err(|e| e.to_string())?;
    planner_tick(&app);
    Ok(())
}

#[tauri::command]
pub fn get_end_action(state: State<'_, AppState>) -> EndAction {
    load_end_action(&state.storage)
}

#[tauri::command]
pub fn set_end_action(state: State<'_, AppState>, a: EndAction) -> Result<(), String> {
    state.storage.set_pref(END_ACTION_KEY, &end_action_str(a)).map_err(|e| e.to_string())
}

/// Exécute l'action de fin de file enregistrée (appelée par l'interface après le compte à rebours).
#[tauri::command]
pub fn run_end_action(app: AppHandle) -> Result<(), String> {
    // Assurance avant un arrêt : si quelque chose a été ajouté ou repris pendant le compte à
    // rebours, ne rien faire.
    let list = app.state::<AppState>().storage.list().map_err(|e| e.to_string())?;
    if !end_action_still_due(&list) {
        return Ok(());
    }
    let action = load_end_action(&app.state::<AppState>().storage);
    if action == EndAction::Quit {
        app.exit(0);
        return Ok(());
    }
    let Some((program, args)) = planner::end_action_command(action) else {
        return Ok(());
    };
    let mut cmd = std::process::Command::new(program);
    cmd.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.spawn().map(|_| ()).map_err(|e| format!("{program} : {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dl(id: &str, status: DownloadStatus) -> Download {
        serde_json::from_value(serde_json::json!({
            "id": id, "url": "https://x/a", "file_name": "a", "dest_path": "a",
            "total_bytes": null, "downloaded_bytes": 0, "status": status,
            "supports_ranges": false, "error": null,
        }))
        .unwrap()
    }

    #[test]
    fn transition_cases() {
        assert_eq!(planner_transition(false, false), Transition::None);
        assert_eq!(planner_transition(true, true), Transition::None);
        assert_eq!(planner_transition(false, true), Transition::Enter);
        assert_eq!(planner_transition(true, false), Transition::Exit);
    }

    #[test]
    fn release_in_window_only_without_transition_inside_window() {
        assert!(release_in_window(Transition::None, true, false));
        assert!(!release_in_window(Transition::None, false, false));
        // Enter lâche déjà les held (et lève la suspension) ; Exit ne lâche rien.
        assert!(!release_in_window(Transition::Enter, true, false));
        assert!(!release_in_window(Transition::Exit, false, false));
        // File suspendue dans la plage : les held restent retenus.
        assert!(!release_in_window(Transition::None, true, true));
    }

    #[test]
    fn startup_exit_only_outside_window_with_marked_items() {
        assert!(startup_exit_needed(false, true));
        assert!(!startup_exit_needed(false, false));
        assert!(!startup_exit_needed(true, true));
        assert!(!startup_exit_needed(true, false));
    }

    #[test]
    fn end_action_only_when_nothing_pending() {
        use DownloadStatus::*;
        assert!(end_action_still_due(&[]));
        assert!(end_action_still_due(&[dl("c", Completed), dl("e", Error)]));
        for s in [Queued, Held, Downloading, Recording, Paused] {
            assert!(!end_action_still_due(&[dl("c", Completed), dl("x", s)]), "{s:?}");
        }
    }

    fn app_state(rows: &[(&str, DownloadStatus)]) -> AppState {
        let storage = Storage::open_in_memory().unwrap();
        for (id, status) in rows {
            storage.upsert(&dl(id, *status)).unwrap();
        }
        AppState::new(storage, 0)
    }

    fn status_of(st: &AppState, id: &str) -> DownloadStatus {
        st.storage.get_by_id(id).unwrap().unwrap().status
    }

    fn pick(st: &AppState) -> Vec<String> {
        download_core::scheduler::pick_to_start(&st.storage.list().unwrap(), 5, None)
    }

    #[test]
    fn pause_queue_holds_waiting_and_records_running() {
        use DownloadStatus::*;
        let st = app_state(&[("run", Downloading), ("wait", Queued), ("mine", Paused), ("done", Completed)]);
        let h = download_core::engine::DownloadHandle::new();
        st.handles.lock().unwrap().insert("run".into(), h.clone());
        suspend_queue(&st).unwrap();
        assert!(st.storage.queue_paused().unwrap());
        assert_eq!(status_of(&st, "wait"), Held);
        assert_eq!(status_of(&st, "mine"), Paused);
        assert_eq!(status_of(&st, "done"), Completed);
        assert!(h.is_paused());
        assert_eq!(sorted(&st.planner.lock().unwrap().suspend_paused), vec!["run".to_string()]);
        // Persisté.
        assert_eq!(load_marks(&st.storage).suspend_paused, vec!["run".to_string()]);
    }

    #[test]
    fn suspended_queue_starts_new_queued_and_resume_all_items() {
        use DownloadStatus::*;
        let st = app_state(&[("run", Downloading), ("wait", Queued)]);
        st.handles.lock().unwrap().insert("run".into(), download_core::engine::DownloadHandle::new());
        suspend_queue(&st).unwrap();
        st.handles.lock().unwrap().clear();
        st.storage.update_state("run", 0, Paused).unwrap(); // la pause du moteur est persistée
        // « Télécharger » pendant la suspension : le nouvel élément démarre, pas les retenus.
        st.storage.upsert(&dl("new", Queued)).unwrap();
        assert_eq!(pick(&st), vec!["new".to_string()]);
        // « Tout reprendre » : les éléments en pause redeviennent élus.
        let ids = crate::commands::requeue_all_paused(&st.storage).unwrap();
        assert_eq!(ids, vec!["run".to_string()]);
        let picked = pick(&st);
        assert!(picked.contains(&"run".to_string()) && picked.contains(&"new".to_string()));
        assert!(!picked.contains(&"wait".to_string()));
    }

    #[test]
    fn start_queue_requeues_suspend_paused_only() {
        use DownloadStatus::*;
        let st = app_state(&[("run", Downloading), ("wait", Queued), ("mine", Paused)]);
        st.handles.lock().unwrap().insert("run".into(), download_core::engine::DownloadHandle::new());
        suspend_queue(&st).unwrap();
        st.handles.lock().unwrap().clear();
        st.storage.update_state("run", 0, Paused).unwrap();
        resume_queue(&st).unwrap();
        assert!(!st.storage.queue_paused().unwrap());
        assert_eq!(status_of(&st, "run"), Queued);
        assert_eq!(status_of(&st, "wait"), Queued);
        // Mis en pause individuellement : reste en pause.
        assert_eq!(status_of(&st, "mine"), Paused);
        assert!(st.planner.lock().unwrap().suspend_paused.is_empty());
        assert!(load_marks(&st.storage).suspend_paused.is_empty());
    }

    #[test]
    fn manual_resume_forgets_suspend_mark() {
        use DownloadStatus::*;
        let st = app_state(&[("run", Downloading)]);
        st.handles.lock().unwrap().insert("run".into(), download_core::engine::DownloadHandle::new());
        suspend_queue(&st).unwrap();
        st.handles.lock().unwrap().clear();
        // Repris à la main puis remis en pause individuellement : « Tout démarrer » le laisse.
        forget_suspend_paused(&st, &["run".to_string()]);
        st.storage.update_state("run", 0, Paused).unwrap();
        resume_queue(&st).unwrap();
        assert_eq!(status_of(&st, "run"), Paused);
    }

    #[test]
    fn marks_persist_and_reload_at_startup() {
        let s = Storage::open_in_memory().unwrap();
        assert_eq!(load_marks(&s), PlannerMarks::default());
        s.set_pref(MARKS_KEY, "pas du json").unwrap();
        assert_eq!(load_marks(&s), PlannerMarks::default());
        let m = PlannerMarks { launched: vec!["a".into(), "b".into()], suspend_paused: vec!["c".into()] };
        write_marks(&s, &m).unwrap();
        assert_eq!(load_marks(&s), m);
        // Champ absent : valeur par défaut.
        s.set_pref(MARKS_KEY, r#"{"launched":["x"]}"#).unwrap();
        assert_eq!(load_marks(&s).launched, vec!["x".to_string()]);
        // Rechargement au démarrage puis persistance d'un changement.
        write_marks(&s, &m).unwrap();
        let st = AppState::new(s, 0);
        load_planner_marks(&st);
        assert_eq!(sorted(&st.planner.lock().unwrap().launched), vec!["a".to_string(), "b".to_string()]);
        assert_eq!(sorted(&st.planner.lock().unwrap().suspend_paused), vec!["c".to_string()]);
        forget_suspend_paused(&st, &["c".to_string()]);
        assert_eq!(load_marks(&st.storage), PlannerMarks { launched: m.launched.clone(), suspend_paused: vec![] });
    }

    #[test]
    fn exit_action_cases() {
        use DownloadStatus::*;
        assert_eq!(exit_action(Queued, false), ExitAction::Hold);
        assert_eq!(exit_action(Queued, true), ExitAction::PauseThenHold);
        assert_eq!(exit_action(Downloading, true), ExitAction::PauseThenHold);
        // Pause utilisateur / « Suspendre la file » : respectée.
        assert_eq!(exit_action(Paused, false), ExitAction::Leave);
        assert_eq!(exit_action(Held, false), ExitAction::Leave);
        assert_eq!(exit_action(Completed, false), ExitAction::Leave);
        assert_eq!(exit_action(Error, false), ExitAction::Leave);
        // Live laissé en cours ; `downloading` sans tâche (état périmé) laissé tel quel.
        assert_eq!(exit_action(Recording, true), ExitAction::Leave);
        assert_eq!(exit_action(Downloading, false), ExitAction::Leave);
    }

    #[test]
    fn pending_counts_waiting_running_and_paused() {
        use DownloadStatus::*;
        let list = vec![
            dl("q", Queued),
            dl("h", Held),
            dl("d", Downloading),
            dl("r", Recording),
            dl("p", Paused),
            dl("e", Error),
            dl("c", Completed),
        ];
        assert_eq!(pending_count(&list), 5);
        assert_eq!(pending_count(&[]), 0);
        assert_eq!(pending_count(&[dl("p", Paused), dl("c", Completed)]), 1);
        assert_eq!(pending_count(&[dl("e", Error), dl("c", Completed)]), 0);
    }

    #[test]
    fn schedule_and_end_action_persist_with_defaults() {
        let s = Storage::open_in_memory().unwrap();
        assert_eq!(load_schedule(&s), Schedule::default());
        assert_eq!(load_end_action(&s), EndAction::None);
        s.set_pref(SCHEDULE_KEY, "pas du json").unwrap();
        assert_eq!(load_schedule(&s), Schedule::default());
        let mut sch = Schedule::default();
        sch.enabled = true;
        sch.limit_bps = Some(1000);
        s.set_pref(SCHEDULE_KEY, &serde_json::to_string(&sch).unwrap()).unwrap();
        assert_eq!(load_schedule(&s), sch);
        s.set_pref(END_ACTION_KEY, &end_action_str(EndAction::Hibernate)).unwrap();
        assert_eq!(s.get_pref(END_ACTION_KEY).unwrap().as_deref(), Some("hibernate"));
        assert_eq!(load_end_action(&s), EndAction::Hibernate);
    }
}
