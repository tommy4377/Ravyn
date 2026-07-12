/// Chooses a conservative number of HTTP ranges from resource size and user limits.
/// More ranges are not always faster: small files and HTTP/2 origins often perform
/// better with fewer concurrent streams.
pub fn adaptive_segment_count(total_bytes: u64, requested: usize, maximum: usize) -> usize {
    let cap = requested.max(1).min(maximum.max(1));
    let by_size = match total_bytes {
        0..=8_388_607 => 1,
        8_388_608..=33_554_431 => 2,
        33_554_432..=134_217_727 => 4,
        134_217_728..=536_870_911 => 6,
        _ => 8,
    };
    cap.min(by_size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_files_use_one_segment() {
        assert_eq!(adaptive_segment_count(1024, 16, 16), 1);
    }

    #[test]
    fn caller_and_global_caps_are_respected() {
        assert_eq!(adaptive_segment_count(1024 * 1024 * 1024, 3, 16), 3);
        assert_eq!(adaptive_segment_count(1024 * 1024 * 1024, 16, 5), 5);
    }
}

/// Adjusts concurrency using persisted host behavior. Repeated range failures
/// reduce parallelism, while consistently fast and reliable hosts may use the
/// full caller-selected limit.
pub fn profile_adjusted_segment_count(
    base: usize,
    consecutive_failures: u32,
    range_failures: u32,
    average_throughput_bps: Option<u64>,
) -> usize {
    let mut value = base.max(1);
    if consecutive_failures >= 2 || range_failures >= 3 {
        value = value.min(2);
    } else if average_throughput_bps.is_some_and(|speed| speed < 2 * 1024 * 1024) {
        value = value.min(3);
    }
    value.max(1)
}

#[cfg(test)]
mod profile_tests {
    use super::*;

    #[test]
    fn unreliable_hosts_are_throttled() {
        assert_eq!(profile_adjusted_segment_count(8, 2, 0, None), 2);
        assert_eq!(profile_adjusted_segment_count(8, 0, 3, None), 2);
    }

    #[test]
    fn slow_hosts_use_fewer_streams() {
        assert_eq!(profile_adjusted_segment_count(8, 0, 0, Some(512_000)), 3);
    }
}
