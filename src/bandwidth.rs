use std::collections::BTreeMap;
use std::fs;
use std::mem;
use std::os::fd::RawFd;
use std::path::Path;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq)]
pub struct EventFormat {
    pub name: String,
    pub start_bit: u8,
    pub end_bit: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PmuEvent {
    pub controller: String,
    pub name: String,
    pub event_type: u32,
    pub cpu: i32,
    pub config: u64,
    pub scale: f64,
    pub unit: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CounterValue {
    pub raw: u64,
    pub scale: f64,
    pub unit: String,
}

impl CounterValue {
    pub fn new(raw: u64, scale: f64, unit: &str) -> Self {
        Self {
            raw,
            scale,
            unit: unit.to_string(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ControllerSample {
    pub controller: String,
    pub read: Option<CounterValue>,
    pub write: Option<CounterValue>,
    pub total: Option<CounterValue>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ControllerBandwidth {
    pub controller: String,
    pub read_mib_s: Option<f64>,
    pub write_mib_s: Option<f64>,
    pub total_mib_s: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct BandwidthSnapshot {
    pub read_mib_s: Option<f64>,
    pub write_mib_s: Option<f64>,
    pub total_mib_s: Option<f64>,
    pub controllers: Vec<ControllerBandwidth>,
}

#[derive(Debug)]
pub struct PmuReader {
    counters: Vec<PerfCounter>,
}

#[derive(Debug)]
struct PerfCounter {
    event: PmuEvent,
    fd: RawFd,
}

impl Drop for PerfCounter {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.fd);
        }
    }
}

pub fn discover_pmu_events(sysfs_root: &Path) -> Vec<PmuEvent> {
    let mut events = Vec::new();

    let Ok(entries) = fs::read_dir(sysfs_root) else {
        return events;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let controller = entry.file_name().to_string_lossy().to_string();
        if !controller.starts_with("uncore_imc_free_running_") || !path.is_dir() {
            continue;
        }

        let Some(event_type) = read_trimmed(&path.join("type")).and_then(|value| parse_u32(&value))
        else {
            continue;
        };
        let cpu = read_trimmed(&path.join("cpumask"))
            .and_then(|value| parse_first_cpu(&value))
            .unwrap_or(0);
        let formats = read_formats(&path.join("format"));

        for event_name in ["data_read", "data_write", "data_total"] {
            let event_path = path.join("events").join(event_name);
            let Some(event_spec) = read_trimmed(&event_path) else {
                continue;
            };
            let Some(config) = pack_config(&event_spec, &formats) else {
                continue;
            };
            let scale = read_trimmed(&event_path.with_file_name(format!("{event_name}.scale")))
                .and_then(|value| value.parse::<f64>().ok())
                .unwrap_or(1.0);
            let unit = read_trimmed(&event_path.with_file_name(format!("{event_name}.unit")))
                .unwrap_or_else(|| "count".to_string());

            events.push(PmuEvent {
                controller: controller.clone(),
                name: event_name.to_string(),
                event_type,
                cpu,
                config,
                scale,
                unit,
            });
        }
    }

    events.sort_by(|left, right| {
        left.controller
            .cmp(&right.controller)
            .then_with(|| left.name.cmp(&right.name))
    });
    events
}

pub fn pack_config(spec: &str, formats: &[EventFormat]) -> Option<u64> {
    let mut config = 0_u64;

    for assignment in spec.split(',').filter(|part| !part.trim().is_empty()) {
        let (name, value) = assignment.trim().split_once('=')?;
        let value = parse_u64(value.trim())?;
        let format = formats.iter().find(|format| format.name == name.trim())?;
        let width = format.end_bit.checked_sub(format.start_bit)? + 1;
        let mask = if width >= 64 {
            u64::MAX
        } else {
            (1_u64 << width) - 1
        };
        config |= (value & mask) << format.start_bit;
    }

    Some(config)
}

pub fn calculate_bandwidth(
    prev: &[ControllerSample],
    curr: &[ControllerSample],
    elapsed: Duration,
) -> Option<BandwidthSnapshot> {
    let elapsed_secs = elapsed.as_secs_f64();
    if elapsed_secs <= 0.0 {
        return None;
    }

    let prev_by_controller = prev
        .iter()
        .map(|sample| (sample.controller.as_str(), sample))
        .collect::<BTreeMap<_, _>>();

    let mut controllers = Vec::new();
    for curr_sample in curr {
        let Some(prev_sample) = prev_by_controller.get(curr_sample.controller.as_str()) else {
            continue;
        };
        let read_mib_s = rate_mib_s(
            prev_sample.read.as_ref(),
            curr_sample.read.as_ref(),
            elapsed_secs,
        );
        let write_mib_s = rate_mib_s(
            prev_sample.write.as_ref(),
            curr_sample.write.as_ref(),
            elapsed_secs,
        );
        let measured_total = rate_mib_s(
            prev_sample.total.as_ref(),
            curr_sample.total.as_ref(),
            elapsed_secs,
        );
        let total_mib_s = measured_total.or_else(|| sum_options([read_mib_s, write_mib_s]));

        controllers.push(ControllerBandwidth {
            controller: curr_sample.controller.clone(),
            read_mib_s,
            write_mib_s,
            total_mib_s,
        });
    }

    if controllers.is_empty() {
        return None;
    }

    Some(BandwidthSnapshot {
        read_mib_s: sum_options(controllers.iter().map(|controller| controller.read_mib_s)),
        write_mib_s: sum_options(controllers.iter().map(|controller| controller.write_mib_s)),
        total_mib_s: sum_options(controllers.iter().map(|controller| controller.total_mib_s)),
        controllers,
    })
}

pub fn rate_mib_s(
    prev: Option<&CounterValue>,
    curr: Option<&CounterValue>,
    elapsed_secs: f64,
) -> Option<f64> {
    if elapsed_secs <= 0.0 {
        return None;
    }

    let prev = prev?;
    let curr = curr?;
    let delta = curr.raw.wrapping_sub(prev.raw);
    Some(delta as f64 * unit_scale_to_mib(curr.scale, &curr.unit)? / elapsed_secs)
}

impl PmuReader {
    pub fn open(sysfs_root: &Path) -> Result<Self, String> {
        let events = discover_pmu_events(sysfs_root);
        if events.is_empty() {
            return Err("no Intel uncore IMC free-running PMU events found".to_string());
        }

        let mut counters = Vec::new();
        for event in events {
            let fd = open_perf_event(&event).map_err(|error| {
                format!(
                    "failed to open {} {}: {error}",
                    event.controller, event.name
                )
            })?;
            counters.push(PerfCounter { event, fd });
        }

        Ok(Self { counters })
    }

    pub fn sample(&self) -> Result<Vec<ControllerSample>, String> {
        let mut by_controller = BTreeMap::<String, ControllerSample>::new();

        for counter in &self.counters {
            let raw = read_counter(counter.fd).map_err(|error| {
                format!(
                    "failed to read {} {}: {error}",
                    counter.event.controller, counter.event.name
                )
            })?;
            let value = CounterValue::new(raw, counter.event.scale, &counter.event.unit);
            let sample = by_controller
                .entry(counter.event.controller.clone())
                .or_insert_with(|| ControllerSample {
                    controller: counter.event.controller.clone(),
                    ..ControllerSample::default()
                });

            match counter.event.name.as_str() {
                "data_read" => sample.read = Some(value),
                "data_write" => sample.write = Some(value),
                "data_total" => sample.total = Some(value),
                _ => {}
            }
        }

        Ok(by_controller.into_values().collect())
    }
}

fn read_formats(format_root: &Path) -> Vec<EventFormat> {
    let mut formats = fs::read_dir(format_root)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.flatten())
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            let value = read_trimmed(&entry.path())?;
            let range = value.strip_prefix("config:")?;
            let (start_bit, end_bit) = parse_bit_range(range)?;
            Some(EventFormat {
                name,
                start_bit,
                end_bit,
            })
        })
        .collect::<Vec<_>>();
    formats.sort_by(|left, right| left.name.cmp(&right.name));
    formats
}

fn parse_bit_range(value: &str) -> Option<(u8, u8)> {
    if let Some((start, end)) = value.split_once('-') {
        Some((start.parse().ok()?, end.parse().ok()?))
    } else {
        let bit = value.parse().ok()?;
        Some((bit, bit))
    }
}

fn parse_first_cpu(value: &str) -> Option<i32> {
    value
        .split(',')
        .next()?
        .split('-')
        .next()?
        .trim()
        .parse()
        .ok()
}

fn read_trimmed(path: &Path) -> Option<String> {
    let value = fs::read_to_string(path).ok()?.trim().to_string();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn parse_u32(value: &str) -> Option<u32> {
    parse_u64(value).and_then(|value| u32::try_from(value).ok())
}

fn parse_u64(value: &str) -> Option<u64> {
    if let Some(hex) = value.strip_prefix("0x") {
        u64::from_str_radix(hex, 16).ok()
    } else {
        value.parse().ok()
    }
}

fn unit_scale_to_mib(scale: f64, unit: &str) -> Option<f64> {
    match unit {
        "MiB" => Some(scale),
        "GiB" => Some(scale * 1024.0),
        "KiB" => Some(scale / 1024.0),
        "B" | "Bytes" | "bytes" => Some(scale / 1024.0 / 1024.0),
        _ => None,
    }
}

fn sum_options(values: impl IntoIterator<Item = Option<f64>>) -> Option<f64> {
    let mut sum = 0.0;
    let mut any = false;
    for value in values.into_iter().flatten() {
        sum += value;
        any = true;
    }
    any.then_some(sum)
}

#[cfg(target_os = "linux")]
fn open_perf_event(event: &PmuEvent) -> std::io::Result<RawFd> {
    let mut attr = PerfEventAttr {
        type_: event.event_type,
        size: mem::size_of::<PerfEventAttr>() as u32,
        config: event.config,
        ..PerfEventAttr::default()
    };

    let fd = unsafe {
        libc::syscall(
            libc::SYS_perf_event_open,
            &mut attr as *mut PerfEventAttr,
            -1_i32,
            event.cpu,
            -1_i32,
            0_u64,
        )
    } as RawFd;

    if fd < 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(fd)
    }
}

#[cfg(not(target_os = "linux"))]
fn open_perf_event(_event: &PmuEvent) -> std::io::Result<RawFd> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "perf_event_open is only available on Linux",
    ))
}

fn read_counter(fd: RawFd) -> std::io::Result<u64> {
    let mut value = 0_u64;
    let read = unsafe {
        libc::read(
            fd,
            &mut value as *mut u64 as *mut libc::c_void,
            mem::size_of::<u64>(),
        )
    };
    if read == mem::size_of::<u64>() as isize {
        Ok(value)
    } else if read < 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            "short perf counter read",
        ))
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct PerfEventAttr {
    type_: u32,
    size: u32,
    config: u64,
    sample_period_or_freq: u64,
    sample_type: u64,
    read_format: u64,
    flags: u64,
    wakeup: u32,
    bp_type: u32,
    config1: u64,
    config2: u64,
}

impl Default for PerfEventAttr {
    fn default() -> Self {
        Self {
            type_: 0,
            size: 0,
            config: 0,
            sample_period_or_freq: 0,
            sample_type: 0,
            read_format: 0,
            flags: 0,
            wakeup: 0,
            bp_type: 0,
            config1: 0,
            config2: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    use std::time::Duration;
    use tempfile::TempDir;

    fn write(path: &Path, value: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, value).unwrap();
    }

    #[test]
    fn discovers_imc_free_running_events_from_sysfs() {
        let temp = TempDir::new().unwrap();
        let device = temp.path().join("uncore_imc_free_running_0");
        write(&device.join("type"), "29\n");
        write(&device.join("cpumask"), "0\n");
        write(&device.join("format/event"), "config:0-7\n");
        write(&device.join("format/umask"), "config:8-15\n");
        write(&device.join("events/data_read"), "event=0xff,umask=0x20\n");
        write(&device.join("events/data_read.scale"), "6.103515625e-5\n");
        write(&device.join("events/data_read.unit"), "MiB\n");
        write(&device.join("events/data_write"), "event=0xff,umask=0x30\n");
        write(&device.join("events/data_write.scale"), "6.103515625e-5\n");
        write(&device.join("events/data_write.unit"), "MiB\n");

        let events = discover_pmu_events(temp.path());

        assert_eq!(events.len(), 2);
        assert_eq!(events[0].controller, "uncore_imc_free_running_0");
        assert_eq!(events[0].name, "data_read");
        assert_eq!(events[0].event_type, 29);
        assert_eq!(events[0].cpu, 0);
        assert_eq!(events[0].config, 0x20ff);
        assert_eq!(events[0].scale, 6.103515625e-5);
        assert_eq!(events[0].unit, "MiB");
    }

    #[test]
    fn packs_event_config_from_format_fields() {
        let formats = vec![
            EventFormat {
                name: "event".to_string(),
                start_bit: 0,
                end_bit: 7,
            },
            EventFormat {
                name: "umask".to_string(),
                start_bit: 8,
                end_bit: 15,
            },
        ];

        assert_eq!(pack_config("event=0xff,umask=0x20", &formats), Some(0x20ff));
    }

    #[test]
    fn calculates_bandwidth_from_counter_deltas() {
        let prev = vec![ControllerSample {
            controller: "imc0".to_string(),
            read: Some(CounterValue::new(1_000, 0.5, "MiB")),
            write: Some(CounterValue::new(2_000, 0.25, "MiB")),
            total: None,
        }];
        let curr = vec![ControllerSample {
            controller: "imc0".to_string(),
            read: Some(CounterValue::new(1_200, 0.5, "MiB")),
            write: Some(CounterValue::new(2_400, 0.25, "MiB")),
            total: None,
        }];

        let bandwidth = calculate_bandwidth(&prev, &curr, Duration::from_secs(2)).unwrap();

        assert_eq!(bandwidth.controllers.len(), 1);
        assert_eq!(bandwidth.controllers[0].read_mib_s, Some(50.0));
        assert_eq!(bandwidth.controllers[0].write_mib_s, Some(50.0));
        assert_eq!(bandwidth.read_mib_s, Some(50.0));
        assert_eq!(bandwidth.write_mib_s, Some(50.0));
        assert_eq!(bandwidth.total_mib_s, Some(100.0));
    }

    #[test]
    fn calculates_counter_wraparound() {
        let prev = CounterValue::new(u64::MAX - 9, 1.0, "MiB");
        let curr = CounterValue::new(10, 1.0, "MiB");

        assert_eq!(rate_mib_s(Some(&prev), Some(&curr), 2.0), Some(10.0));
    }
}
