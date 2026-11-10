//! File d'attente et planificateur : commandes, boucle de vérification, action de fin.
//!
//! Ordre des verrous : `handles` → `planner`. Le verrou `planner` est toujours une feuille :
//! aucune autre ressource (stockage, `handles`, émission, `reevaluate`) n'est prise tant qu'il
//! est tenu.

use crate::commands::{focus_main_window, reevaluate};
use crate::state::AppState;
use download_core::model::{Download, DownloadStatus};
use download_core::planner::{self, AtEnd, EndAction, RunTracker, Schedule};
use download_core::storage::Storage;
use std::collections::HashSet;
use tauri::{AppHandle, Emitter, Manager, State};

const SCHEDULE_KEY: &str = "schedule";
const END_ACTION_KEY: &str = "end_action";
/// Période de la boucle du planificateur.
const TICK_SECS: u64 = 30;

/// État en mémoire du planificateur et du suivi de fin de file.
#[derive(Default)]
pub struct PlannerState {
    /// Dernier état connu « dans la plage horaire ».
    pub in_window: bool,
    /// Ids passés de `held` à `queued` par le planificateur à l'entrée dans la plage.
    pub launched: HashSet<String>,
    /// Exécution en cours (détection de la fin de file).
    pub run: RunTracker,
    /// Ids mis en pause à la sortie de la plage : leur `Paused` est persisté en `Held`.
    pub hold_on_pause: HashSet<String>,
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

/// Éléments encore « à faire » : en attente (`queued`, `held`) ou en cours (`downloading`, `recording`).
pub fn pending_count(list: &[Download]) -> usize {
    list.iter()
        .filter(|d| {
            matches!(
                d.status,
                DownloadStatus::Queued | DownloadStatus::Held | DownloadStatus::Downloading | DownloadStatus::Recording
            )
        })
        .count()
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

// --- Hooks appelés depuis commands.rs ---

/// Un téléchargement vient de passer en cours (`mark_active` dans `reevaluate`).
pub(crate) fn on_started(app: &AppHandle) {
    app.state::<AppState>().planner.lock().unwrap().run.on_started();
}

/// Retire `id` de la liste « à remettre en attente » ; `true` s'il y figurait.
pub(crate) fn take_hold_on_pause(app: &AppHandle, id: &str) -> bool {
    app.state::<AppState>().planner.lock().unwrap().hold_on_pause.remove(id)
}

/// À appeler APRÈS la persistance d'un statut terminal. `first` = première persistance de ce
/// statut pour ce téléchargement (évite de compter deux fois un même `Completed`).
/// Sur `Completed`/`Error`, émet `queue://finished` si l'exécution s'est vidée avec au moins un
/// succès et qu'une action de fin est choisie. `Paused`/`Held` (suspension voulue par
/// l'utilisateur ou le planificateur) ne déclenchent pas la fin de file.
pub(crate) fn on_terminal_persisted(app: &AppHandle, status: DownloadStatus, first: bool) {
    let st = app.state::<AppState>();
    if status == DownloadStatus::Completed && first {
        st.planner.lock().unwrap().run.on_completed();
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
    let finished = st.planner.lock().unwrap().run.check_finished(pending);
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

/// Un passage du planificateur à l'heure locale courante.
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
    let (transition, launched) = {
        let mut p = st.planner.lock().unwrap();
        let t = planner_transition(p.in_window, now_in);
        p.in_window = now_in;
        let launched = if t == Transition::Exit { std::mem::take(&mut p.launched) } else { HashSet::new() };
        (t, launched)
    };
    match transition {
        Transition::None => {}
        Transition::Enter => enter_window(app, &schedule),
        Transition::Exit => exit_window(app, &schedule, launched),
    }
}

fn enter_window(app: &AppHandle, schedule: &Schedule) {
    let st = app.state::<AppState>();
    let ids = match release_held(&st.storage) {
        Ok(ids) => ids,
        Err(e) => {
            eprintln!("planificateur : passage en file impossible : {e}");
            Vec::new()
        }
    };
    st.planner.lock().unwrap().launched.extend(ids);
    if let Some(limit) = schedule.limit_bps {
        st.limiter.set_rate(limit);
    }
    if let Err(e) = st.storage.set_queue_paused(false) {
        eprintln!("planificateur : reprise de la file impossible : {e}");
    }
    reevaluate(app);
}

fn exit_window(app: &AppHandle, schedule: &Schedule, launched: HashSet<String>) {
    let st = app.state::<AppState>();
    if schedule.at_end == AtEnd::Pause {
        for id in launched {
            let d = match st.storage.get_by_id(&id) {
                Ok(Some(d)) => d,
                _ => continue,
            };
            if matches!(d.status, DownloadStatus::Completed | DownloadStatus::Error) {
                continue;
            }
            // `handles` tenu pendant la décision : un démarrage concurrent ne peut pas s'intercaler.
            let handles = st.handles.lock().unwrap();
            if let Some(h) = handles.get(&id) {
                // Actif : pause ; le relais persistera `Held` au lieu de `Paused`.
                st.planner.lock().unwrap().hold_on_pause.insert(id.clone());
                h.pause();
            } else if let Err(e) = st.storage.update_state(&id, d.downloaded_bytes, DownloadStatus::Held) {
                eprintln!("planificateur : mise en attente de {id} impossible : {e}");
            }
        }
    }
    let max = st.storage.get_settings().map(|s| s.max_speed_bps).unwrap_or(0);
    st.limiter.set_rate(max);
}

/// Boucle du planificateur : un passage immédiat puis toutes les 30 s.
pub(crate) fn start_planner_loop(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(TICK_SECS));
        loop {
            // Le premier `tick` est immédiat. Aucun verrou n'est tenu à travers l'`await`.
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

#[tauri::command]
pub fn start_queue(app: AppHandle) -> Result<(), String> {
    {
        let st = app.state::<AppState>();
        st.storage.set_queue_paused(false).map_err(|e| e.to_string())?;
        release_held(&st.storage)?;
    }
    reevaluate(&app);
    Ok(())
}

#[tauri::command]
pub fn pause_queue(state: State<'_, AppState>) -> Result<(), String> {
    state.storage.set_queue_paused(true).map_err(|e| e.to_string())?;
    pause_active(&state);
    Ok(())
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
    fn pending_counts_waiting_and_running_only() {
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
        assert_eq!(pending_count(&list), 4);
        assert_eq!(pending_count(&[]), 0);
        assert_eq!(pending_count(&[dl("p", Paused), dl("c", Completed)]), 0);
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
