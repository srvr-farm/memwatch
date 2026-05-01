use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MemoryInfo {
    pub total_bytes: u64,
    pub free_bytes: u64,
    pub available_bytes: u64,
    pub buffers_bytes: u64,
    pub cached_bytes: u64,
    pub sreclaimable_bytes: u64,
    pub shmem_bytes: u64,
    pub swap_total_bytes: u64,
    pub swap_free_bytes: u64,
    pub dirty_bytes: u64,
    pub writeback_bytes: u64,
    pub anon_bytes: u64,
    pub slab_bytes: u64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct MemorySummary {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub free_bytes: u64,
    pub buffers_bytes: u64,
    pub cache_bytes: u64,
    pub swap_total_bytes: u64,
    pub swap_used_bytes: u64,
    pub dirty_bytes: u64,
    pub writeback_bytes: u64,
    pub anon_bytes: u64,
    pub slab_bytes: u64,
    pub used_percent: Option<f64>,
}

pub fn read_meminfo(path: &Path) -> MemoryInfo {
    fs::read_to_string(path)
        .map(|input| parse_meminfo(&input))
        .unwrap_or_default()
}

pub fn parse_meminfo(input: &str) -> MemoryInfo {
    let mut info = MemoryInfo::default();

    for line in input.lines() {
        let Some((key, rest)) = line.split_once(':') else {
            continue;
        };
        let Some(kib) = rest
            .split_whitespace()
            .next()
            .and_then(|value| value.parse::<u64>().ok())
        else {
            continue;
        };
        let bytes = kib.saturating_mul(1024);

        match key {
            "MemTotal" => info.total_bytes = bytes,
            "MemFree" => info.free_bytes = bytes,
            "MemAvailable" => info.available_bytes = bytes,
            "Buffers" => info.buffers_bytes = bytes,
            "Cached" => info.cached_bytes = bytes,
            "SReclaimable" => info.sreclaimable_bytes = bytes,
            "Shmem" => info.shmem_bytes = bytes,
            "SwapTotal" => info.swap_total_bytes = bytes,
            "SwapFree" => info.swap_free_bytes = bytes,
            "Dirty" => info.dirty_bytes = bytes,
            "Writeback" => info.writeback_bytes = bytes,
            "AnonPages" => info.anon_bytes = bytes,
            "Slab" => info.slab_bytes = bytes,
            _ => {}
        }
    }

    info
}

pub fn summarize(info: &MemoryInfo) -> MemorySummary {
    let used_bytes = info.total_bytes.saturating_sub(info.available_bytes);
    let cache_bytes = info
        .cached_bytes
        .saturating_add(info.sreclaimable_bytes)
        .saturating_sub(info.shmem_bytes);
    let swap_used_bytes = info.swap_total_bytes.saturating_sub(info.swap_free_bytes);
    let used_percent = (info.total_bytes > 0)
        .then_some((used_bytes as f64 / info.total_bytes as f64) * 100.0);

    MemorySummary {
        total_bytes: info.total_bytes,
        used_bytes,
        available_bytes: info.available_bytes,
        free_bytes: info.free_bytes,
        buffers_bytes: info.buffers_bytes,
        cache_bytes,
        swap_total_bytes: info.swap_total_bytes,
        swap_used_bytes,
        dirty_bytes: info.dirty_bytes,
        writeback_bytes: info.writeback_bytes,
        anon_bytes: info.anon_bytes,
        slab_bytes: info.slab_bytes,
        used_percent,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MEMINFO: &str = "\
MemTotal:       1000000 kB
MemFree:         100000 kB
MemAvailable:    700000 kB
Buffers:          20000 kB
Cached:          300000 kB
SwapCached:           0 kB
Active:          200000 kB
Inactive:        300000 kB
Shmem:            50000 kB
SwapTotal:       200000 kB
SwapFree:        150000 kB
Dirty:             4000 kB
Writeback:         1000 kB
AnonPages:       250000 kB
Slab:             80000 kB
SReclaimable:     30000 kB
";

    #[test]
    fn parses_meminfo_values_as_bytes() {
        let info = parse_meminfo(MEMINFO);

        assert_eq!(info.total_bytes, 1_000_000 * 1024);
        assert_eq!(info.free_bytes, 100_000 * 1024);
        assert_eq!(info.available_bytes, 700_000 * 1024);
        assert_eq!(info.buffers_bytes, 20_000 * 1024);
        assert_eq!(info.cached_bytes, 300_000 * 1024);
        assert_eq!(info.sreclaimable_bytes, 30_000 * 1024);
        assert_eq!(info.shmem_bytes, 50_000 * 1024);
        assert_eq!(info.swap_total_bytes, 200_000 * 1024);
        assert_eq!(info.swap_free_bytes, 150_000 * 1024);
        assert_eq!(info.dirty_bytes, 4_000 * 1024);
        assert_eq!(info.writeback_bytes, 1_000 * 1024);
        assert_eq!(info.anon_bytes, 250_000 * 1024);
        assert_eq!(info.slab_bytes, 80_000 * 1024);
    }

    #[test]
    fn summarizes_memory_usage() {
        let summary = summarize(&parse_meminfo(MEMINFO));

        assert_eq!(summary.used_bytes, 300_000 * 1024);
        assert_eq!(summary.cache_bytes, 280_000 * 1024);
        assert_eq!(summary.swap_used_bytes, 50_000 * 1024);
        assert_eq!(summary.used_percent, Some(30.0));
    }
}
