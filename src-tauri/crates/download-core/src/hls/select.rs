use crate::hls::playlist::{AudioRendition, MasterPlaylist, Variant};

/// Index de la meilleure variante (plus haut BANDWIDTH). 0 si vide.
pub fn best_variant(m: &MasterPlaylist) -> usize {
    m.variants
        .iter()
        .enumerate()
        .max_by_key(|(_, v)| v.bandwidth)
        .map(|(i, _)| i)
        .unwrap_or(0)
}

/// Taille estimée en octets : bitrate (bits/s) × durée (s) ÷ 8.
pub fn estimated_size(bandwidth: u64, total_duration_secs: f64) -> u64 {
    ((bandwidth as f64) * total_duration_secs / 8.0) as u64
}

/// Piste audio à utiliser pour une variante : la rendition DEFAULT (sinon AUTOSELECT,
/// sinon la première) du groupe audio de la variante, à condition qu'elle ait une URI.
pub fn pick_audio<'a>(m: &'a MasterPlaylist, variant: &Variant) -> Option<&'a AudioRendition> {
    let group = variant.audio_group.as_ref()?;
    let in_group: Vec<&AudioRendition> =
        m.audio.iter().filter(|a| &a.group_id == group && a.uri.is_some()).collect();
    in_group
        .iter()
        .find(|a| a.default)
        .or_else(|| in_group.iter().find(|a| a.autoselect))
        .or_else(|| in_group.first())
        .copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hls::playlist::{AudioRendition, MasterPlaylist, Variant};

    fn variant(bw: u64, group: Option<&str>) -> Variant {
        Variant { uri: "u".into(), bandwidth: bw, resolution: None, audio_group: group.map(String::from) }
    }

    #[test]
    fn best_variant_picks_highest_bandwidth() {
        let m = MasterPlaylist {
            variants: vec![variant(800_000, None), variant(2_400_000, None), variant(1_200_000, None)],
            audio: vec![],
        };
        assert_eq!(best_variant(&m), 1);
    }

    #[test]
    fn estimated_size_uses_bitrate_times_duration() {
        // 8_000_000 bits/s × 10 s ÷ 8 = 10_000_000 octets
        assert_eq!(estimated_size(8_000_000, 10.0), 10_000_000);
    }

    #[test]
    fn pick_audio_prefers_default() {
        let m = MasterPlaylist {
            variants: vec![],
            audio: vec![
                AudioRendition { group_id: "aud".into(), name: "FR".into(), uri: Some("fr.m3u8".into()), default: false, autoselect: true },
                AudioRendition { group_id: "aud".into(), name: "EN".into(), uri: Some("en.m3u8".into()), default: true, autoselect: true },
            ],
        };
        let v = variant(1, Some("aud"));
        assert_eq!(pick_audio(&m, &v).unwrap().name, "EN");
    }

    #[test]
    fn pick_audio_none_when_no_group() {
        let m = MasterPlaylist::default();
        assert!(pick_audio(&m, &variant(1, None)).is_none());
    }
}
