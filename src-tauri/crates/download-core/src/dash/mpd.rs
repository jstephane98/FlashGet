use crate::hls::playlist::resolve_uri;
use crate::CoreError;
use roxmltree::Document;

#[derive(Debug, Clone, PartialEq)]
pub enum ContentType { Video, Audio, Other }

#[derive(Debug, Clone, PartialEq)]
pub enum ProtScheme { Cenc, ClearKey, Other }

#[derive(Debug, Clone, PartialEq)]
pub struct ContentProtection {
    pub default_kid: Option<String>,
    pub clearkey_laurl: Option<String>,
    pub scheme: ProtScheme,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TimelineS { pub t: Option<u64>, pub d: u64, pub r: i64 }

#[derive(Debug, Clone, PartialEq)]
pub struct SegmentTemplate {
    pub init: Option<String>,
    pub media: String,
    pub timescale: u64,
    pub duration: Option<u64>,
    pub start_number: u64,
    pub timeline: Option<Vec<TimelineS>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SegmentListEntry { pub media: String, pub range: Option<(u64, u64)> }

#[derive(Debug, Clone, PartialEq)]
pub struct SegmentList {
    pub init: Option<String>,
    pub init_range: Option<(u64, u64)>,
    pub segments: Vec<SegmentListEntry>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SegmentBase {
    /// URL du fichier unique de la Representation (BaseURL résolue).
    pub url: String,
    pub init_range: Option<(u64, u64)>,
    pub index_range: Option<(u64, u64)>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Addressing { Template(SegmentTemplate), List(SegmentList), Base(SegmentBase) }

#[derive(Debug, Clone, PartialEq)]
pub struct Representation {
    pub id: String,
    pub bandwidth: u64,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub mime: Option<String>,
    /// BaseURL effective de la Representation (pour résoudre les templates/listes relatifs).
    pub base: String,
    pub addressing: Addressing,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AdaptationSet {
    pub content_type: ContentType,
    pub lang: Option<String>,
    pub representations: Vec<Representation>,
    pub content_protection: Option<ContentProtection>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Period { pub adaptation_sets: Vec<AdaptationSet> }

#[derive(Debug, Clone, PartialEq)]
pub struct Mpd {
    pub dynamic: bool,
    /// Durée totale (s) depuis MPD@mediaPresentationDuration (0 si absente).
    pub duration_secs: f64,
    /// MPD@minimumUpdatePeriod en secondes (cadence de refresh du live), si présent.
    pub minimum_update_period: Option<f64>,
    pub periods: Vec<Period>,
}

/// Parse un manifest MPD. Les URLs sont résolues contre `base_url`.
pub fn parse_mpd(xml: &str, base_url: &str) -> Result<Mpd, CoreError> {
    let doc = Document::parse(xml).map_err(|e| CoreError::Media(format!("MPD invalide: {e}")))?;
    let root = doc.root_element();
    let dynamic = attr(&root, "type").as_deref() == Some("dynamic");
    let duration_secs = attr(&root, "mediaPresentationDuration")
        .and_then(|s| parse_iso8601_duration(&s))
        .unwrap_or(0.0);
    let minimum_update_period = attr(&root, "minimumUpdatePeriod").and_then(|s| parse_iso8601_duration(&s));

    let mpd_base = child_base_url(&root, base_url);

    let mut periods = Vec::new();
    for pnode in root.children().filter(|n| n.has_tag_name_local("Period")) {
        let period_base = child_base_url(&pnode, &mpd_base);
        let mut adaptation_sets = Vec::new();
        for asnode in pnode.children().filter(|n| n.has_tag_name_local("AdaptationSet")) {
            let as_base = child_base_url(&asnode, &period_base);
            let content_type = parse_content_type(&asnode);
            let lang = attr(&asnode, "lang");
            let content_protection = parse_content_protection(&asnode);
            let as_template = asnode
                .children()
                .find(|n| n.has_tag_name_local("SegmentTemplate"))
                .map(|n| parse_segment_template(&n, &as_base));

            let mut representations = Vec::new();
            for rnode in asnode.children().filter(|n| n.has_tag_name_local("Representation")) {
                let rep_base = child_base_url(&rnode, &as_base);
                let id = attr(&rnode, "id").unwrap_or_default();
                let bandwidth = attr(&rnode, "bandwidth").and_then(|s| s.parse().ok()).unwrap_or(0);
                let width = attr(&rnode, "width").and_then(|s| s.parse().ok());
                let height = attr(&rnode, "height").and_then(|s| s.parse().ok());
                let mime = attr(&rnode, "mimeType");
                let addressing = parse_addressing(&rnode, &rep_base, &as_base, as_template.as_ref())?;
                representations.push(Representation { id, bandwidth, width, height, mime, base: rep_base, addressing });
            }
            adaptation_sets.push(AdaptationSet { content_type, lang, representations, content_protection });
        }
        periods.push(Period { adaptation_sets });
    }
    Ok(Mpd { dynamic, duration_secs, minimum_update_period, periods })
}

/// Adressage d'une Representation : SegmentBase/SegmentList/SegmentTemplate au niveau
/// Representation, sinon le SegmentTemplate hérité de l'AdaptationSet.
fn parse_addressing(
    rnode: &roxmltree::Node,
    rep_base: &str,
    _as_base: &str,
    as_template: Option<&SegmentTemplate>,
) -> Result<Addressing, CoreError> {
    if let Some(b) = rnode.children().find(|n| n.has_tag_name_local("SegmentBase")) {
        return Ok(Addressing::Base(SegmentBase {
            url: rep_base.to_string(),
            init_range: b
                .children()
                .find(|n| n.has_tag_name_local("Initialization"))
                .and_then(|i| attr(&i, "range").as_deref().and_then(parse_range)),
            index_range: attr(&b, "indexRange").as_deref().and_then(parse_range),
        }));
    }
    if let Some(l) = rnode.children().find(|n| n.has_tag_name_local("SegmentList")) {
        let init_node = l.children().find(|n| n.has_tag_name_local("Initialization"));
        let init = init_node.as_ref().and_then(|i| attr(i, "sourceURL"));
        let init_range = init_node.as_ref().and_then(|i| attr(i, "range").as_deref().and_then(parse_range));
        let segments = l
            .children()
            .filter(|n| n.has_tag_name_local("SegmentURL"))
            .map(|s| SegmentListEntry {
                media: attr(&s, "media").unwrap_or_default(),
                range: attr(&s, "mediaRange").as_deref().and_then(parse_range),
            })
            .collect();
        return Ok(Addressing::List(SegmentList { init, init_range, segments }));
    }
    if let Some(t) = rnode.children().find(|n| n.has_tag_name_local("SegmentTemplate")) {
        return Ok(Addressing::Template(parse_segment_template(&t, rep_base)));
    }
    if let Some(t) = as_template {
        return Ok(Addressing::Template(t.clone()));
    }
    Err(CoreError::Media("Representation sans adressage de segments".into()))
}

fn parse_segment_template(n: &roxmltree::Node, _base: &str) -> SegmentTemplate {
    let timeline = n.children().find(|c| c.has_tag_name_local("SegmentTimeline")).map(|tl| {
        tl.children()
            .filter(|s| s.has_tag_name_local("S"))
            .map(|s| TimelineS {
                t: attr(&s, "t").and_then(|v| v.parse().ok()),
                d: attr(&s, "d").and_then(|v| v.parse().ok()).unwrap_or(0),
                r: attr(&s, "r").and_then(|v| v.parse().ok()).unwrap_or(0),
            })
            .collect()
    });
    SegmentTemplate {
        init: attr(n, "initialization"),
        media: attr(n, "media").unwrap_or_default(),
        timescale: attr(n, "timescale").and_then(|s| s.parse().ok()).unwrap_or(1),
        duration: attr(n, "duration").and_then(|s| s.parse().ok()),
        start_number: attr(n, "startNumber").and_then(|s| s.parse().ok()).unwrap_or(1),
        timeline,
    }
}

fn parse_content_type(asnode: &roxmltree::Node) -> ContentType {
    if let Some(ct) = attr(asnode, "contentType") {
        return match ct.as_str() {
            "video" => ContentType::Video,
            "audio" => ContentType::Audio,
            _ => ContentType::Other,
        };
    }
    match attr(asnode, "mimeType") {
        Some(m) if m.starts_with("video/") => ContentType::Video,
        Some(m) if m.starts_with("audio/") => ContentType::Audio,
        _ => ContentType::Other,
    }
}

fn parse_content_protection(asnode: &roxmltree::Node) -> Option<ContentProtection> {
    let cps: Vec<roxmltree::Node> =
        asnode.children().filter(|n| n.has_tag_name_local("ContentProtection")).collect();
    if cps.is_empty() {
        return None;
    }
    let mut default_kid = None;
    let mut clearkey_laurl = None;
    let mut scheme = ProtScheme::Other;
    for cp in &cps {
        if let Some(kid) = cp.attributes().find(|a| a.name() == "default_KID").map(|a| a.value().to_string()) {
            default_kid = Some(kid);
        }
        let s = attr(cp, "schemeIdUri").unwrap_or_default();
        if s == "urn:mpeg:dash:mp4protection:2011" {
            scheme = ProtScheme::Cenc;
        }
        if s.eq_ignore_ascii_case("urn:uuid:e2719d58-a985-b3c9-781a-b030af78d30e") {
            scheme = ProtScheme::ClearKey;
            if let Some(la) = cp.children().find(|n| {
                n.is_element() && n.tag_name().name().eq_ignore_ascii_case("laurl")
            }) {
                clearkey_laurl = la.text().map(|t| t.trim().to_string());
            }
        }
    }
    Some(ContentProtection { default_kid, clearkey_laurl, scheme })
}

fn child_base_url(node: &roxmltree::Node, parent_base: &str) -> String {
    match node.children().find(|n| n.has_tag_name_local("BaseURL")).and_then(|b| b.text()) {
        Some(b) => resolve_uri(parent_base, b.trim()),
        None => parent_base.to_string(),
    }
}

fn attr(n: &roxmltree::Node, name: &str) -> Option<String> {
    n.attribute(name).map(|s| s.to_string())
}

fn parse_range(s: &str) -> Option<(u64, u64)> {
    let (a, b) = s.split_once('-')?;
    Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
}

/// Parse une durée ISO-8601 limitée (`PT#H#M#S`, secondes fractionnaires) en secondes.
fn parse_iso8601_duration(s: &str) -> Option<f64> {
    let s = s.strip_prefix('P')?;
    let t = s.split_once('T').map(|(_, t)| t).unwrap_or(s);
    let mut secs = 0.0;
    let mut num = String::new();
    for c in t.chars() {
        if c.is_ascii_digit() || c == '.' {
            num.push(c);
        } else {
            let v: f64 = num.parse().ok()?;
            num.clear();
            match c {
                'H' => secs += v * 3600.0,
                'M' => secs += v * 60.0,
                'S' => secs += v,
                _ => {}
            }
        }
    }
    Some(secs)
}

/// Extension d'aide : test du nom local d'une balise (ignore le namespace).
trait LocalName {
    fn has_tag_name_local(&self, name: &str) -> bool;
}
impl LocalName for roxmltree::Node<'_, '_> {
    fn has_tag_name_local(&self, name: &str) -> bool {
        self.is_element() && self.tag_name().name() == name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEMPLATE_NUMBER: &str = r#"<?xml version="1.0"?>
<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" type="static" mediaPresentationDuration="PT10S">
  <Period>
    <AdaptationSet contentType="video" mimeType="video/mp4">
      <SegmentTemplate initialization="init-$RepresentationID$.mp4" media="seg-$RepresentationID$-$Number$.m4s" timescale="1000" duration="2000" startNumber="1"/>
      <Representation id="v0" bandwidth="800000" width="640" height="360"/>
      <Representation id="v1" bandwidth="2400000" width="1280" height="720"/>
    </AdaptationSet>
    <AdaptationSet contentType="audio" lang="en" mimeType="audio/mp4">
      <SegmentTemplate initialization="init-$RepresentationID$.mp4" media="seg-$RepresentationID$-$Number$.m4s" timescale="1000" duration="2000" startNumber="1"/>
      <Representation id="a0" bandwidth="128000"/>
    </AdaptationSet>
  </Period>
</MPD>"#;

    #[test]
    fn parses_static_template_mpd() {
        let m = parse_mpd(TEMPLATE_NUMBER, "https://h/p/manifest.mpd").unwrap();
        assert!(!m.dynamic);
        assert_eq!(m.duration_secs, 10.0);
        assert_eq!(m.periods.len(), 1);
        let video = &m.periods[0].adaptation_sets[0];
        assert_eq!(video.content_type, ContentType::Video);
        assert_eq!(video.representations.len(), 2);
        assert_eq!(video.representations[1].bandwidth, 2_400_000);
        assert_eq!(video.representations[1].width, Some(1280));
        let audio = &m.periods[0].adaptation_sets[1];
        assert_eq!(audio.content_type, ContentType::Audio);
        assert_eq!(audio.lang.as_deref(), Some("en"));
        match &video.representations[0].addressing {
            Addressing::Template(t) => {
                assert_eq!(t.media, "seg-$RepresentationID$-$Number$.m4s");
                assert_eq!(t.duration, Some(2000));
                assert_eq!(t.timescale, 1000);
            }
            other => panic!("attendu Template, obtenu {other:?}"),
        }
    }

    #[test]
    fn parses_segment_list_with_init_range() {
        let xml = r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" type="static" mediaPresentationDuration="PT4S">
  <Period><AdaptationSet contentType="video">
    <Representation id="v0" bandwidth="1"><BaseURL>https://h/p/</BaseURL>
      <SegmentList>
        <Initialization sourceURL="init.mp4" range="0-99"/>
        <SegmentURL media="s1.m4s" mediaRange="100-199"/>
        <SegmentURL media="s2.m4s"/>
      </SegmentList>
    </Representation>
  </AdaptationSet></Period>
</MPD>"#;
        let m = parse_mpd(xml, "https://h/p/m.mpd").unwrap();
        match &m.periods[0].adaptation_sets[0].representations[0].addressing {
            Addressing::List(l) => {
                assert_eq!(l.init.as_deref(), Some("init.mp4"));
                assert_eq!(l.init_range, Some((0, 99)));
                assert_eq!(l.segments.len(), 2);
                assert_eq!(l.segments[0].range, Some((100, 199)));
                assert_eq!(l.segments[1].range, None);
            }
            other => panic!("attendu List, obtenu {other:?}"),
        }
    }

    #[test]
    fn detects_dynamic_and_clearkey() {
        let xml = r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" xmlns:cenc="urn:mpeg:cenc:2013" type="dynamic">
  <Period><AdaptationSet contentType="video">
    <ContentProtection schemeIdUri="urn:mpeg:dash:mp4protection:2011" cenc:default_KID="abc"/>
    <ContentProtection schemeIdUri="urn:uuid:e2719d58-a985-b3c9-781a-b030af78d30e">
      <clearkey:Laurl xmlns:clearkey="http://dashif.org/guidelines/clearKey">https://lic/ck</clearkey:Laurl>
    </ContentProtection>
    <Representation id="v0" bandwidth="1"><BaseURL>v.mp4</BaseURL><SegmentBase indexRange="0-99"/></Representation>
  </AdaptationSet></Period>
</MPD>"#;
        let m = parse_mpd(xml, "https://h/p/m.mpd").unwrap();
        assert!(m.dynamic);
        let cp = m.periods[0].adaptation_sets[0].content_protection.as_ref().unwrap();
        assert_eq!(cp.default_kid.as_deref(), Some("abc"));
        assert_eq!(cp.clearkey_laurl.as_deref(), Some("https://lic/ck"));
        match &m.periods[0].adaptation_sets[0].representations[0].addressing {
            Addressing::Base(b) => {
                assert_eq!(b.url, "https://h/p/v.mp4");
                assert_eq!(b.index_range, Some((0, 99)));
            }
            other => panic!("attendu Base, obtenu {other:?}"),
        }
    }

    #[test]
    fn parses_minimum_update_period_for_dynamic() {
        let xml = r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" type="dynamic" minimumUpdatePeriod="PT2S"><Period/></MPD>"#;
        let m = parse_mpd(xml, "https://h/p/m.mpd").unwrap();
        assert!(m.dynamic);
        assert_eq!(m.minimum_update_period, Some(2.0));
    }

    #[test]
    fn minimum_update_period_absent_is_none() {
        let xml = r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" type="static" mediaPresentationDuration="PT4S"><Period/></MPD>"#;
        let m = parse_mpd(xml, "https://h/p/m.mpd").unwrap();
        assert_eq!(m.minimum_update_period, None);
    }

    #[test]
    fn clearkey_laurl_lowercase_dashif() {
        let xml = r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" type="static">
  <Period><AdaptationSet contentType="video">
    <ContentProtection schemeIdUri="urn:uuid:e2719d58-a985-b3c9-781a-b030af78d30e">
      <dashif:laurl xmlns:dashif="https://dashif.org/CPS">https://lic/ck</dashif:laurl>
    </ContentProtection>
    <Representation id="v0" bandwidth="1"><BaseURL>v.mp4</BaseURL><SegmentBase indexRange="0-99"/></Representation>
  </AdaptationSet></Period>
</MPD>"#;
        let m = parse_mpd(xml, "https://h/p/m.mpd").unwrap();
        let cp = m.periods[0].adaptation_sets[0].content_protection.as_ref().unwrap();
        assert_eq!(cp.clearkey_laurl.as_deref(), Some("https://lic/ck"));
    }
}
