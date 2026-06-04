use crate::dash::mpd::{ContentType, Mpd};

/// Coordonnées d'une Representation dans le MPD.
#[derive(Debug, Clone, PartialEq)]
pub struct ReprCoord { pub period: usize, pub adaptation: usize, pub representation: usize }

/// Variante affichable côté UI (même forme que `MediaVariant` côté Tauri).
#[derive(Debug, Clone, PartialEq)]
pub struct DashVariant {
    pub index: u32,
    pub resolution: Option<String>,
    pub bandwidth: u64,
    pub est_size: Option<u64>,
}

/// Représentations vidéo de la 1ère Period, dans l'ordre, avec leurs coordonnées.
pub fn video_variants(mpd: &Mpd) -> Vec<(ReprCoord, DashVariant)> {
    let mut out = Vec::new();
    if let Some(period) = mpd.periods.first() {
        for (ai, aset) in period.adaptation_sets.iter().enumerate() {
            if aset.content_type != ContentType::Video {
                continue;
            }
            for (ri, r) in aset.representations.iter().enumerate() {
                let coord = ReprCoord { period: 0, adaptation: ai, representation: ri };
                let est = if r.bandwidth > 0 && mpd.duration_secs > 0.0 {
                    Some(estimated_size(r.bandwidth, mpd.duration_secs))
                } else {
                    None
                };
                out.push((coord, DashVariant {
                    index: out.len() as u32,
                    resolution: match (r.width, r.height) {
                        (Some(w), Some(h)) => Some(format!("{w}x{h}")),
                        _ => None,
                    },
                    bandwidth: r.bandwidth,
                    est_size: est,
                }));
            }
        }
    }
    out
}

/// Index de la meilleure variante vidéo (plus haut bitrate). None si aucune.
pub fn best_video_index(mpd: &Mpd) -> Option<u32> {
    video_variants(mpd).into_iter().max_by_key(|(_, v)| v.bandwidth).map(|(_, v)| v.index)
}

/// Coordonnées de l'AdaptationSet audio (langue demandée sinon première) + sa
/// meilleure Representation. None si pas d'audio.
pub fn pick_audio(mpd: &Mpd, lang: Option<&str>) -> Option<ReprCoord> {
    let period = mpd.periods.first()?;
    let audio: Vec<(usize, &_)> = period
        .adaptation_sets
        .iter()
        .enumerate()
        .filter(|(_, a)| a.content_type == ContentType::Audio && !a.representations.is_empty())
        .collect();
    let chosen = lang
        .and_then(|l| audio.iter().find(|(_, a)| a.lang.as_deref() == Some(l)))
        .or_else(|| audio.first())?;
    let (ai, aset) = (chosen.0, chosen.1);
    let ri = aset
        .representations
        .iter()
        .enumerate()
        .max_by_key(|(_, r)| r.bandwidth)
        .map(|(i, _)| i)?;
    Some(ReprCoord { period: 0, adaptation: ai, representation: ri })
}

pub fn estimated_size(bandwidth: u64, total_duration_secs: f64) -> u64 {
    ((bandwidth as f64) * total_duration_secs / 8.0) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dash::mpd::*;

    fn rep(id: &str, bw: u64, w: Option<u32>, h: Option<u32>) -> Representation {
        Representation {
            id: id.into(), bandwidth: bw, width: w, height: h, mime: None, base: String::new(),
            addressing: Addressing::Template(SegmentTemplate {
                init: None, media: "$Number$".into(), timescale: 1, duration: Some(1), start_number: 1, timeline: None,
            }),
        }
    }
    fn aset(ct: ContentType, lang: Option<&str>, reps: Vec<Representation>) -> AdaptationSet {
        AdaptationSet { content_type: ct, lang: lang.map(String::from), representations: reps, content_protection: None }
    }

    fn mpd() -> Mpd {
        Mpd {
            dynamic: false, duration_secs: 10.0,
            periods: vec![Period { adaptation_sets: vec![
                aset(ContentType::Video, None, vec![rep("v0", 800000, Some(640), Some(360)), rep("v1", 2400000, Some(1280), Some(720))]),
                aset(ContentType::Audio, Some("en"), vec![rep("a0", 128000, None, None)]),
                aset(ContentType::Audio, Some("fr"), vec![rep("a1", 128000, None, None)]),
            ] }],
        }
    }

    #[test]
    fn video_variants_and_best() {
        let m = mpd();
        let vs = video_variants(&m);
        assert_eq!(vs.len(), 2);
        assert_eq!(vs[1].1.resolution.as_deref(), Some("1280x720"));
        assert_eq!(vs[1].1.est_size, Some(estimated_size(2400000, 10.0)));
        assert_eq!(best_video_index(&m), Some(1));
    }

    #[test]
    fn pick_audio_by_lang_then_first() {
        let m = mpd();
        assert_eq!(pick_audio(&m, Some("fr")).unwrap().adaptation, 2);
        assert_eq!(pick_audio(&m, None).unwrap().adaptation, 1);
        assert_eq!(pick_audio(&m, Some("de")).unwrap().adaptation, 1); // langue absente => premier
    }
}
