use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub struct ProcessMemory {
    pub pid: u32,
    pub name: String,
    pub uid: Option<u32>,
    pub rss_bytes: u64,
    pub rss_percent: Option<f64>,
}

impl ProcessMemory {
    pub fn new(pid: u32, name: String, uid: Option<u32>, rss_bytes: u64) -> Self {
        Self {
            pid,
            name,
            uid,
            rss_bytes,
            rss_percent: None,
        }
    }
}

pub fn parse_status(input: &str) -> Option<ProcessMemory> {
    let mut name = None;
    let mut pid = None;
    let mut uid = None;
    let mut rss_bytes = None;

    for line in input.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        match key {
            "Name" => name = Some(value.to_string()),
            "Pid" => pid = value.parse::<u32>().ok(),
            "Uid" => uid = value.split_whitespace().next()?.parse::<u32>().ok(),
            "VmRSS" => {
                rss_bytes = value
                    .split_whitespace()
                    .next()
                    .and_then(|value| value.parse::<u64>().ok())
                    .map(|kib| kib.saturating_mul(1024));
            }
            _ => {}
        }
    }

    Some(ProcessMemory::new(pid?, name?, uid, rss_bytes?))
}

pub fn scan_proc(proc_root: &Path, total_memory_bytes: u64, limit: usize) -> Vec<ProcessMemory> {
    let processes = fs::read_dir(proc_root)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.flatten())
        .filter_map(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .parse::<u32>()
                .ok()
                .map(|_| entry.path().join("status"))
        })
        .filter_map(|path| fs::read_to_string(path).ok())
        .filter_map(|input| parse_status(&input))
        .collect::<Vec<_>>();

    top_by_rss(processes, total_memory_bytes, limit)
}

pub fn top_by_rss(
    mut processes: Vec<ProcessMemory>,
    total_memory_bytes: u64,
    limit: usize,
) -> Vec<ProcessMemory> {
    processes.sort_by(|left, right| {
        right
            .rss_bytes
            .cmp(&left.rss_bytes)
            .then_with(|| left.pid.cmp(&right.pid))
    });
    processes.truncate(limit);

    for process in &mut processes {
        process.rss_percent = (total_memory_bytes > 0)
            .then_some((process.rss_bytes as f64 / total_memory_bytes as f64) * 100.0);
    }

    processes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_proc_status_memory_fields() {
        let status = "\
Name:\tpostgres
Umask:\t0022
State:\tS (sleeping)
Pid:\t4242
Uid:\t1000\t1000\t1000\t1000
VmRSS:\t  123456 kB
";

        let process = parse_status(status).unwrap();

        assert_eq!(process.pid, 4242);
        assert_eq!(process.name, "postgres");
        assert_eq!(process.uid, Some(1000));
        assert_eq!(process.rss_bytes, 123456 * 1024);
    }

    #[test]
    fn sorts_and_limits_top_rss_processes() {
        let processes = vec![
            ProcessMemory::new(1, "small".to_string(), None, 10),
            ProcessMemory::new(2, "large".to_string(), Some(1000), 80),
            ProcessMemory::new(3, "medium".to_string(), Some(1000), 40),
        ];

        let top = top_by_rss(processes, 200, 2);

        assert_eq!(top.len(), 2);
        assert_eq!(top[0].pid, 2);
        assert_eq!(top[0].rss_percent, Some(40.0));
        assert_eq!(top[1].pid, 3);
        assert_eq!(top[1].rss_percent, Some(20.0));
    }
}
