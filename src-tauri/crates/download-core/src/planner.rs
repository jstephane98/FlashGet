//! Logique pure du planificateur : plage horaire, prochain démarrage,
//! action de fin de file et détection de fin d'exécution.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AtEnd {
    #[default]
    Pause,
    Continue,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum EndAction {
    #[default]
    None,
    Quit,
    Sleep,
    Hibernate,
    Shutdown,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Schedule {
    pub enabled: bool,
    pub start: String,
    pub end: String,
    /// Index 0 = lundi.
    pub days: [bool; 7],
    pub limit_bps: Option<u64>,
    pub at_end: AtEnd,
}

impl Default for Schedule {
    fn default() -> Self {
        Schedule {
            enabled: false,
            start: "02:00".into(),
            end: "07:00".into(),
            days: [true, true, true, true, true, false, false],
            limit_bps: None,
            at_end: AtEnd::Pause,
        }
    }
}

/// "HH:MM" strict (deux chiffres chacun) -> minutes 0..1439.
pub fn parse_hhmm(s: &str) -> Option<u16> {
    let b = s.as_bytes();
    if b.len() != 5 || b[2] != b':' {
        return None;
    }
    if ![0, 1, 3, 4].iter().all(|&i| b[i].is_ascii_digit()) {
        return None;
    }
    let h = ((b[0] - b'0') * 10 + (b[1] - b'0')) as u16;
    let m = ((b[3] - b'0') * 10 + (b[4] - b'0')) as u16;
    if h > 23 || m > 59 {
        return None;
    }
    Some(h * 60 + m)
}

pub fn validate(s: &Schedule) -> Result<(), String> {
    let start = parse_hhmm(&s.start).ok_or_else(|| format!("heure de début invalide : {}", s.start))?;
    let end = parse_hhmm(&s.end).ok_or_else(|| format!("heure de fin invalide : {}", s.end))?;
    if start == end {
        return Err("l'heure de début et l'heure de fin doivent être différentes".into());
    }
    if s.enabled && !s.days.iter().any(|d| *d) {
        return Err("au moins un jour doit être coché".into());
    }
    Ok(())
}

/// `weekday` : 0 = lundi. Une plage qui passe minuit appartient au jour où elle commence.
pub fn in_window(s: &Schedule, weekday: u8, minute: u16) -> bool {
    if !s.enabled || weekday > 6 {
        return false;
    }
    let (Some(start), Some(end)) = (parse_hhmm(&s.start), parse_hhmm(&s.end)) else {
        return false;
    };
    let today = s.days[weekday as usize];
    if start < end {
        today && minute >= start && minute < end
    } else if start > end {
        let yesterday = s.days[((weekday + 6) % 7) as usize];
        (today && minute >= start) || (yesterday && minute < end)
    } else {
        false
    }
}

/// Premier début strictement après maintenant : (jours d'écart 0..=7, minute).
pub fn next_start(s: &Schedule, weekday: u8, minute: u16) -> Option<(u8, u16)> {
    if !s.enabled || weekday > 6 {
        return None;
    }
    let start = parse_hhmm(&s.start)?;
    for d in 0..=7u8 {
        let day = (weekday + d) % 7;
        if s.days[day as usize] && (d > 0 || start > minute) {
            return Some((d, start));
        }
    }
    None
}

pub fn end_action_command(a: EndAction) -> Option<(&'static str, Vec<&'static str>)> {
    match a {
        EndAction::Sleep => Some(("rundll32.exe", vec!["powrprof.dll,SetSuspendState", "0,1,0"])),
        EndAction::Hibernate => Some(("shutdown", vec!["/h"])),
        EndAction::Shutdown => Some(("shutdown", vec!["/s", "/t", "0"])),
        EndAction::None | EndAction::Quit => None,
    }
}

#[derive(Default)]
pub struct RunTracker {
    active: bool,
    completed: u32,
}

impl RunTracker {
    pub fn on_started(&mut self) {
        self.active = true;
    }

    pub fn on_completed(&mut self) {
        if self.active {
            self.completed += 1;
        }
    }

    /// Abandonne l'exécution en cours sans signaler de fin (ex. file vidée par annulation).
    pub fn reset(&mut self) {
        self.active = false;
        self.completed = 0;
    }

    /// `true` une seule fois, quand une exécution active s'est vidée avec au moins un succès.
    pub fn check_finished(&mut self, pending: usize) -> bool {
        if pending != 0 || !self.active {
            return false;
        }
        let finished = self.completed > 0;
        self.active = false;
        self.completed = 0;
        finished
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sched(start: &str, end: &str, days: [bool; 7]) -> Schedule {
        Schedule { enabled: true, start: start.into(), end: end.into(), days, limit_bps: None, at_end: AtEnd::Pause }
    }

    #[test]
    fn parse_hhmm_cases() {
        assert_eq!(parse_hhmm("02:00"), Some(120));
        assert_eq!(parse_hhmm("23:59"), Some(1439));
        assert_eq!(parse_hhmm("24:00"), None);
        assert_eq!(parse_hhmm("7:5"), None);
    }

    #[test]
    fn in_window_same_day() {
        let mut s = Schedule::default();
        s.enabled = true;
        assert!(in_window(&s, 0, 180));
        assert!(!in_window(&s, 0, 420));
        assert!(!in_window(&s, 5, 180));
        s.enabled = false;
        assert!(!in_window(&s, 0, 180));
    }

    #[test]
    fn in_window_overnight() {
        let s = sched("22:00", "06:00", [false, false, false, false, true, false, false]);
        assert!(in_window(&s, 4, 23 * 60 + 30));
        assert!(in_window(&s, 5, 180));
        assert!(!in_window(&s, 5, 420));
        assert!(!in_window(&s, 3, 23 * 60 + 30));
    }

    #[test]
    fn next_start_cases() {
        let mut s = Schedule::default();
        s.enabled = true;
        assert_eq!(next_start(&s, 0, 60), Some((0, 120)));
        assert_eq!(next_start(&s, 0, 180), Some((1, 120)));
        assert_eq!(next_start(&s, 4, 180), Some((3, 120)));
        s.days = [false; 7];
        assert_eq!(next_start(&s, 0, 60), None);
    }

    #[test]
    fn validate_cases() {
        assert!(validate(&Schedule::default()).is_ok());
        assert!(validate(&sched("02:00", "02:00", [true; 7])).is_err());
        assert!(validate(&sched("25:00", "02:00", [true; 7])).is_err());
        assert!(validate(&sched("02:00", "07:00", [false; 7])).is_err());
    }

    #[test]
    fn end_action_commands() {
        assert_eq!(end_action_command(EndAction::Sleep).unwrap().0, "rundll32.exe");
        assert_eq!(end_action_command(EndAction::Hibernate), Some(("shutdown", vec!["/h"])));
        assert_eq!(end_action_command(EndAction::Shutdown), Some(("shutdown", vec!["/s", "/t", "0"])));
        assert!(end_action_command(EndAction::None).is_none());
        assert!(end_action_command(EndAction::Quit).is_none());
    }

    #[test]
    fn end_of_queue_detection() {
        let mut r = RunTracker::default();
        assert!(!r.check_finished(0));
        r.on_started();
        assert!(!r.check_finished(1));
        r.on_completed();
        assert!(r.check_finished(0));
        assert!(!r.check_finished(0));
        r.on_started();
        assert!(!r.check_finished(0));
    }

    #[test]
    fn reset_forgets_previous_success() {
        let mut r = RunTracker::default();
        r.on_started();
        r.on_completed();
        r.reset();
        // Nouvelle exécution qui échoue : l'ancien succès ne doit pas déclencher la fin.
        r.on_started();
        assert!(!r.check_finished(0));
        // Reset hors exécution : sans effet.
        r.reset();
        assert!(!r.check_finished(0));
    }

    #[test]
    fn schedule_json_roundtrip() {
        let s = Schedule::default();
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(v["at_end"], "pause");
        assert_eq!(v["days"].as_array().unwrap().len(), 7);
        let back: Schedule = serde_json::from_value(v).unwrap();
        assert_eq!(back, s);
    }
}
