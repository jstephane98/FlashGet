use crate::dash::mpd::{Addressing, Representation, SegmentBase, SegmentList, SegmentTemplate};
use crate::hls::playlist::resolve_uri;

#[derive(Debug, Clone, PartialEq)]
pub struct SegmentRef {
    pub url: String,
    pub byte_range: Option<(u64, u64)>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SegmentPlan {
    pub init: Option<SegmentRef>,
    pub media: Vec<SegmentRef>,
}

/// Construit le plan de segments d'une Representation. Pour SegmentBase, `media`
/// est laissé vide (rempli à l'exécution après lecture du sidx via `base_media`).
pub fn build_segments(repr: &Representation, base: &str, total_duration_secs: f64) -> SegmentPlan {
    match &repr.addressing {
        Addressing::Template(t) => build_template(repr, t, base, total_duration_secs),
        Addressing::List(l) => build_list(l, base),
        Addressing::Base(b) => SegmentPlan {
            init: Some(SegmentRef { url: b.url.clone(), byte_range: b.init_range }),
            media: Vec::new(),
        },
    }
}

/// Médias d'un SegmentBase à partir des ranges issus du sidx.
pub fn base_media(b: &SegmentBase, sidx_ranges: &[(u64, u64)]) -> Vec<SegmentRef> {
    sidx_ranges
        .iter()
        .map(|r| SegmentRef { url: b.url.clone(), byte_range: Some(*r) })
        .collect()
}

fn build_template(repr: &Representation, t: &SegmentTemplate, base: &str, total: f64) -> SegmentPlan {
    let init = t.init.as_ref().map(|tpl| SegmentRef {
        url: resolve_uri(base, &expand(tpl, &repr.id, repr.bandwidth, 0, 0)),
        byte_range: None,
    });
    let mut media = Vec::new();
    if let Some(timeline) = &t.timeline {
        let mut number = t.start_number;
        let mut time = 0u64;
        for s in timeline {
            if let Some(tt) = s.t {
                time = tt;
            }
            let repeat = if s.r < 0 { 0 } else { s.r as u64 }; // r=-1 (illimité) non géré en VOD
            for _ in 0..=repeat {
                media.push(SegmentRef {
                    url: resolve_uri(base, &expand(&t.media, &repr.id, repr.bandwidth, number, time)),
                    byte_range: None,
                });
                number += 1;
                time += s.d;
            }
        }
    } else if let Some(d) = t.duration {
        let count = if d == 0 { 0 } else {
            ((total * t.timescale as f64) / d as f64).ceil() as u64
        };
        for i in 0..count {
            let number = t.start_number + i;
            let time = i * d;
            media.push(SegmentRef {
                url: resolve_uri(base, &expand(&t.media, &repr.id, repr.bandwidth, number, time)),
                byte_range: None,
            });
        }
    }
    SegmentPlan { init, media }
}

fn build_list(l: &SegmentList, base: &str) -> SegmentPlan {
    let init = l.init.as_ref().map(|u| SegmentRef { url: resolve_uri(base, u), byte_range: l.init_range });
    let media = l
        .segments
        .iter()
        .map(|s| SegmentRef { url: resolve_uri(base, &s.media), byte_range: s.range })
        .collect();
    SegmentPlan { init, media }
}

/// Substitution des identifiants de template DASH.
fn expand(template: &str, rep_id: &str, bandwidth: u64, number: u64, time: u64) -> String {
    let mut out = String::new();
    let bytes = template.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'$' {
            out.push(bytes[i] as char);
            i += 1;
            continue;
        }
        let end = match template[i + 1..].find('$') {
            Some(e) => i + 1 + e,
            None => { out.push('$'); break; }
        };
        let tok = &template[i + 1..end];
        if tok.is_empty() {
            out.push('$'); // $$ => $
        } else {
            let (name, fmt) = match tok.split_once('%') {
                Some((n, f)) => (n, Some(f)),
                None => (tok, None),
            };
            let val: u64 = match name {
                "Number" => number,
                "Time" => time,
                "Bandwidth" => bandwidth,
                "RepresentationID" => { out.push_str(rep_id); i = end + 1; continue; }
                _ => { out.push_str(&template[i..=end]); i = end + 1; continue; }
            };
            match fmt {
                Some(f) => out.push_str(&format_padded(val, f)),
                None => out.push_str(&val.to_string()),
            }
        }
        i = end + 1;
    }
    out
}

/// Format minimal `0Nd` (zéro-padding). Ex. "05d" => largeur 5.
fn format_padded(val: u64, fmt: &str) -> String {
    let digits: String = fmt.chars().filter(|c| c.is_ascii_digit()).collect();
    let width: usize = digits.trim_start_matches('0').parse().unwrap_or_else(|_| digits.parse().unwrap_or(0));
    format!("{val:0width$}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dash::mpd::{Addressing, Representation, SegmentTemplate, TimelineS};

    fn repr(addr: Addressing) -> Representation {
        Representation { id: "v0".into(), bandwidth: 800000, width: None, height: None, mime: None, base: String::new(), addressing: addr }
    }

    #[test]
    fn expands_number_template_by_duration() {
        let t = SegmentTemplate {
            init: Some("init-$RepresentationID$.mp4".into()),
            media: "seg-$Number%05d$.m4s".into(),
            timescale: 1000,
            duration: Some(2000),
            start_number: 1,
            timeline: None,
        };
        let plan = build_segments(&repr(Addressing::Template(t)), "https://h/p/", 6.0);
        assert_eq!(plan.init.unwrap().url, "https://h/p/init-v0.mp4");
        // 6 s, segments de 2 s => 3 segments.
        assert_eq!(plan.media.len(), 3);
        assert_eq!(plan.media[0].url, "https://h/p/seg-00001.m4s");
        assert_eq!(plan.media[2].url, "https://h/p/seg-00003.m4s");
    }

    #[test]
    fn expands_timeline() {
        let t = SegmentTemplate {
            init: None,
            media: "$Time$.m4s".into(),
            timescale: 1000,
            duration: None,
            start_number: 1,
            timeline: Some(vec![TimelineS { t: Some(0), d: 1000, r: 2 }]),
        };
        let plan = build_segments(&repr(Addressing::Template(t)), "https://h/p/", 0.0);
        // r=2 => 3 segments aux temps 0,1000,2000.
        assert_eq!(plan.media.iter().map(|s| s.url.clone()).collect::<Vec<_>>(),
            vec!["https://h/p/0.m4s", "https://h/p/1000.m4s", "https://h/p/2000.m4s"]);
    }

    #[test]
    fn base_media_from_sidx() {
        let b = crate::dash::mpd::SegmentBase { url: "https://h/p/v.mp4".into(), init_range: Some((0, 99)), index_range: Some((100, 199)) };
        let refs = base_media(&b, &[(200, 299), (300, 399)]);
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0], SegmentRef { url: "https://h/p/v.mp4".into(), byte_range: Some((200, 299)) });
    }
}
