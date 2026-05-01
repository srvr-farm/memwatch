use crate::bandwidth::{self, BandwidthSnapshot, ControllerSample, PmuReader};
use crate::dmi::{self, MemoryDetails};
use crate::memory::{self, MemorySummary};
use crate::processes::{self, ProcessMemory};
use std::path::PathBuf;
use std::time::Instant;

const DEFAULT_PROCESS_LIMIT: usize = 12;

#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub memory: MemorySummary,
    pub dmi: MemoryDetails,
    pub bandwidth: Option<BandwidthSnapshot>,
    pub processes: Vec<ProcessMemory>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Default)]
pub struct BandwidthTracker {
    previous_samples: Option<Vec<ControllerSample>>,
    previous_at: Option<Instant>,
}

impl BandwidthTracker {
    pub fn update(
        &mut self,
        samples: Vec<ControllerSample>,
        now: Instant,
    ) -> Option<BandwidthSnapshot> {
        let snapshot = match (&self.previous_samples, self.previous_at) {
            (Some(previous_samples), Some(previous_at)) => {
                bandwidth::calculate_bandwidth(previous_samples, &samples, now - previous_at)
            }
            _ => None,
        };

        self.previous_samples = Some(samples);
        self.previous_at = Some(now);
        snapshot
    }
}

#[derive(Debug)]
pub struct Sampler {
    meminfo_path: PathBuf,
    proc_root: PathBuf,
    dmi: MemoryDetails,
    dmi_diagnostic: Option<String>,
    bandwidth_reader: Option<PmuReader>,
    bandwidth_diagnostic: Option<String>,
    bandwidth_tracker: BandwidthTracker,
    process_limit: usize,
}

impl Default for Sampler {
    fn default() -> Self {
        let (dmi, dmi_diagnostic) = dmi::collect();
        let bandwidth_root = PathBuf::from("/sys/bus/event_source/devices");
        let (bandwidth_reader, bandwidth_diagnostic) = match PmuReader::open(&bandwidth_root) {
            Ok(reader) => (Some(reader), None),
            Err(error) => (None, Some(error)),
        };

        Self {
            meminfo_path: PathBuf::from("/proc/meminfo"),
            proc_root: PathBuf::from("/proc"),
            dmi,
            dmi_diagnostic,
            bandwidth_reader,
            bandwidth_diagnostic,
            bandwidth_tracker: BandwidthTracker::default(),
            process_limit: DEFAULT_PROCESS_LIMIT,
        }
    }
}

impl Sampler {
    pub fn new_for_tests(meminfo_path: PathBuf, proc_root: PathBuf, dmi: MemoryDetails) -> Self {
        Self {
            meminfo_path,
            proc_root,
            dmi,
            dmi_diagnostic: None,
            bandwidth_reader: None,
            bandwidth_diagnostic: None,
            bandwidth_tracker: BandwidthTracker::default(),
            process_limit: DEFAULT_PROCESS_LIMIT,
        }
    }

    pub fn sample(&mut self) -> Snapshot {
        self.sample_at(Instant::now())
    }

    fn sample_at(&mut self, now: Instant) -> Snapshot {
        let memory_info = memory::read_meminfo(&self.meminfo_path);
        let memory = memory::summarize(&memory_info);
        let processes = processes::scan_proc(&self.proc_root, memory.total_bytes, self.process_limit);
        let mut diagnostics = Vec::new();

        if memory.total_bytes == 0 {
            diagnostics.push(format!(
                "meminfo unavailable or unreadable at {}",
                self.meminfo_path.display()
            ));
        }
        if let Some(diagnostic) = &self.dmi_diagnostic {
            diagnostics.push(diagnostic.clone());
        }
        if let Some(diagnostic) = &self.bandwidth_diagnostic {
            diagnostics.push(diagnostic.clone());
        }

        let bandwidth = self
            .bandwidth_reader
            .as_ref()
            .and_then(|reader| match reader.sample() {
                Ok(samples) => self.bandwidth_tracker.update(samples, now),
                Err(error) => {
                    diagnostics.push(error);
                    None
                }
            });

        Snapshot {
            memory,
            dmi: self.dmi.clone(),
            bandwidth,
            processes,
            diagnostics,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bandwidth::{ControllerSample, CounterValue};
    use crate::dmi::MemoryDetails;
    use std::fs;
    use std::path::Path;
    use std::time::{Duration, Instant};
    use tempfile::TempDir;

    fn write(path: &Path, value: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, value).unwrap();
    }

    #[test]
    fn bandwidth_tracker_requires_previous_sample() {
        let mut tracker = BandwidthTracker::default();
        let first = vec![ControllerSample {
            controller: "imc0".to_string(),
            read: Some(CounterValue::new(100, 1.0, "MiB")),
            write: None,
            total: None,
        }];
        let second = vec![ControllerSample {
            controller: "imc0".to_string(),
            read: Some(CounterValue::new(160, 1.0, "MiB")),
            write: None,
            total: None,
        }];
        let now = Instant::now();

        assert_eq!(tracker.update(first, now), None);

        let snapshot = tracker.update(second, now + Duration::from_secs(3)).unwrap();
        assert_eq!(snapshot.read_mib_s, Some(20.0));
    }

    #[test]
    fn sampler_combines_memory_dmi_and_process_snapshots() {
        let temp = TempDir::new().unwrap();
        let meminfo = temp.path().join("meminfo");
        let proc_root = temp.path().join("proc");
        write(
            &meminfo,
            "\
MemTotal:       1000000 kB
MemFree:         100000 kB
MemAvailable:    700000 kB
Buffers:          20000 kB
Cached:          300000 kB
SReclaimable:     30000 kB
Shmem:            50000 kB
SwapTotal:       200000 kB
SwapFree:        150000 kB
Dirty:             4000 kB
Writeback:         1000 kB
AnonPages:       250000 kB
Slab:             80000 kB
",
        );
        write(
            &proc_root.join("42/status"),
            "\
Name:\tworker
Pid:\t42
Uid:\t1000\t1000\t1000\t1000
VmRSS:\t100000 kB
",
        );
        let dmi = MemoryDetails {
            total_installed_bytes: 64 * 1024 * 1024 * 1024,
            configured_speed_mts: Some(5600),
            devices: vec![],
        };

        let mut sampler = Sampler::new_for_tests(meminfo, proc_root, dmi);
        let snapshot = sampler.sample();

        assert_eq!(snapshot.memory.used_bytes, 300_000 * 1024);
        assert_eq!(snapshot.dmi.configured_speed_mts, Some(5600));
        assert_eq!(snapshot.processes.len(), 1);
        assert_eq!(snapshot.processes[0].pid, 42);
        assert_eq!(snapshot.bandwidth, None);
    }
}
