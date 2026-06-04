use crate::CoreError;

/// Parse une boîte `sidx` (Segment Index, ISO BMFF) et renvoie les byte-ranges
/// absolus (start, end inclusif) de chaque sous-segment. `anchor` = position
/// absolue de l'octet juste APRÈS la boîte sidx (point d'ancrage de first_offset).
pub fn parse_sidx(b: &[u8], anchor: u64) -> Result<Vec<(u64, u64)>, CoreError> {
    if b.len() < 12 {
        return Err(CoreError::Media("sidx tronqué".into()));
    }
    // On suppose que `b` commence à la boîte sidx (size@0, type@4).
    if &b[4..8] != b"sidx" {
        return Err(CoreError::Media("boîte sidx absente".into()));
    }
    let version = b[8];
    let mut p = 12usize; // après version(1)+flags(3)
    p += 4; // reference_ID
    p += 4; // timescale
    if version == 0 {
        if b.len() < 28 {
            return Err(CoreError::Media("sidx tronqué (entête v0)".into()));
        }
        p += 4; // earliest_presentation_time
        let first_offset = u32::from_be_bytes(b[p..p + 4].try_into().unwrap()) as u64;
        p += 4;
        read_refs(b, p, anchor + first_offset)
    } else {
        if b.len() < 36 {
            return Err(CoreError::Media("sidx tronqué (entête v1)".into()));
        }
        p += 8; // earliest_presentation_time (64)
        let first_offset = u64::from_be_bytes(b[p..p + 8].try_into().unwrap());
        p += 8;
        read_refs(b, p, anchor + first_offset)
    }
}

fn read_refs(b: &[u8], mut p: usize, mut start: u64) -> Result<Vec<(u64, u64)>, CoreError> {
    p += 2; // reserved
    if p + 2 > b.len() {
        return Err(CoreError::Media("sidx tronqué (count)".into()));
    }
    let count = u16::from_be_bytes(b[p..p + 2].try_into().unwrap()) as usize;
    p += 2;
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        if p + 12 > b.len() {
            return Err(CoreError::Media("sidx tronqué (refs)".into()));
        }
        let w = u32::from_be_bytes(b[p..p + 4].try_into().unwrap());
        let referenced_size = (w & 0x7FFF_FFFF) as u64; // bit de poids fort = reference_type
        if referenced_size == 0 {
            return Err(CoreError::Media("sidx: référence de taille nulle".into()));
        }
        p += 12; // referenced_size(4) + subsegment_duration(4) + SAP(4)
        out.push((start, start + referenced_size - 1));
        start += referenced_size;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_two_references() {
        // sidx version 0, 2 références (tailles 100 et 200), first_offset=0.
        let mut b = Vec::new();
        b.extend_from_slice(&56u32.to_be_bytes()); // size
        b.extend_from_slice(b"sidx"); // type
        b.push(0); // version
        b.extend_from_slice(&[0, 0, 0]); // flags
        b.extend_from_slice(&1u32.to_be_bytes()); // reference_ID
        b.extend_from_slice(&1000u32.to_be_bytes()); // timescale
        b.extend_from_slice(&0u32.to_be_bytes()); // earliest_presentation_time
        b.extend_from_slice(&0u32.to_be_bytes()); // first_offset
        b.extend_from_slice(&0u16.to_be_bytes()); // reserved
        b.extend_from_slice(&2u16.to_be_bytes()); // reference_count
        // ref 1
        b.extend_from_slice(&100u32.to_be_bytes());
        b.extend_from_slice(&1000u32.to_be_bytes());
        b.extend_from_slice(&0x9000_0000u32.to_be_bytes());
        // ref 2
        b.extend_from_slice(&200u32.to_be_bytes());
        b.extend_from_slice(&1000u32.to_be_bytes());
        b.extend_from_slice(&0x9000_0000u32.to_be_bytes());
        assert_eq!(b.len(), 56);

        let ranges = parse_sidx(&b, 56).unwrap();
        assert_eq!(ranges, vec![(56, 155), (156, 355)]);
    }

    #[test]
    fn rejects_non_sidx() {
        let b = vec![0u8; 12];
        assert!(parse_sidx(&b, 0).is_err());
    }

    #[test]
    fn rejects_truncated_header() {
        // fourcc 'sidx' valide mais corps trop court (< 28 octets pour v0).
        let mut b = Vec::new();
        b.extend_from_slice(&20u32.to_be_bytes());
        b.extend_from_slice(b"sidx");
        b.push(0); // version 0
        b.extend_from_slice(&[0, 0, 0]); // flags
        b.extend_from_slice(&1u32.to_be_bytes()); // reference_ID
        // s'arrête ici (16 octets) — doit renvoyer Err, pas paniquer
        assert!(parse_sidx(&b, 16).is_err());
    }

    #[test]
    fn masks_reference_type_bit() {
        // une référence de type 1 (bit 31 positionné) de taille 100.
        let mut b = Vec::new();
        b.extend_from_slice(&44u32.to_be_bytes()); // size
        b.extend_from_slice(b"sidx");
        b.push(0);
        b.extend_from_slice(&[0, 0, 0]);
        b.extend_from_slice(&1u32.to_be_bytes()); // reference_ID
        b.extend_from_slice(&1000u32.to_be_bytes()); // timescale
        b.extend_from_slice(&0u32.to_be_bytes()); // earliest_presentation_time
        b.extend_from_slice(&0u32.to_be_bytes()); // first_offset
        b.extend_from_slice(&0u16.to_be_bytes()); // reserved
        b.extend_from_slice(&1u16.to_be_bytes()); // reference_count
        b.extend_from_slice(&0x8000_0064u32.to_be_bytes()); // type=1, size=100
        b.extend_from_slice(&1000u32.to_be_bytes());
        b.extend_from_slice(&0x9000_0000u32.to_be_bytes());
        let ranges = parse_sidx(&b, 44).unwrap();
        assert_eq!(ranges, vec![(44, 143)]); // size masqué à 100
    }
}
