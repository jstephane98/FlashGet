/// Découpe `total` octets en au plus `parts` segments de tailles quasi égales.
/// Retourne des intervalles INCLUSIFS [(start, end)]. Le reste est réparti
/// sur les premiers segments. `parts` est borné à [1, total].
pub fn compute_segments(total: u64, parts: u32) -> Vec<(u64, u64)> {
    if total == 0 {
        return Vec::new();
    }
    let parts = (parts as u64).clamp(1, total);
    let base = total / parts;
    let remainder = total % parts;
    let mut ranges = Vec::with_capacity(parts as usize);
    let mut start = 0u64;
    for i in 0..parts {
        let extra = if i < remainder { 1 } else { 0 };
        let len = base + extra;
        let end = start + len - 1;
        ranges.push((start, end));
        start = end + 1;
    }
    ranges
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn divides_evenly() {
        assert_eq!(compute_segments(100, 4), vec![(0, 24), (25, 49), (50, 74), (75, 99)]);
    }

    #[test]
    fn distributes_remainder_to_first_segments() {
        // 10 / 3 -> longueurs 4,3,3
        assert_eq!(compute_segments(10, 3), vec![(0, 3), (4, 6), (7, 9)]);
    }

    #[test]
    fn zero_total_yields_nothing() {
        assert_eq!(compute_segments(0, 4), Vec::<(u64, u64)>::new());
    }

    #[test]
    fn parts_capped_at_total() {
        // 5 octets, 10 parts demandées -> 5 segments d'un octet
        assert_eq!(compute_segments(5, 10), vec![(0, 0), (1, 1), (2, 2), (3, 3), (4, 4)]);
    }

    #[test]
    fn last_segment_reaches_end() {
        let segs = compute_segments(1000, 7);
        assert_eq!(segs.last().unwrap().1, 999);
        assert_eq!(segs.first().unwrap().0, 0);
    }
}
