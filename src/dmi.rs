use std::process::Command;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MemoryDevice {
    pub locator: Option<String>,
    pub bank_locator: Option<String>,
    pub size_bytes: u64,
    pub memory_type: Option<String>,
    pub speed_mts: Option<u64>,
    pub configured_speed_mts: Option<u64>,
    pub manufacturer: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MemoryDetails {
    pub total_installed_bytes: u64,
    pub configured_speed_mts: Option<u64>,
    pub devices: Vec<MemoryDevice>,
}

#[derive(Debug, Default)]
struct DeviceBuilder {
    seen: bool,
    device: MemoryDevice,
    installed: bool,
}

pub fn parse_dmidecode_memory(input: &str) -> Vec<MemoryDevice> {
    let mut devices = Vec::new();
    let mut current = DeviceBuilder::default();

    for line in input.lines() {
        let line = line.trim();
        if line == "Memory Device" {
            push_current(&mut devices, &mut current);
            current.seen = true;
            continue;
        }

        if !current.seen {
            continue;
        }

        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();

        match key.trim() {
            "Size" => {
                if let Some(size_bytes) = parse_size_bytes(value) {
                    current.device.size_bytes = size_bytes;
                    current.installed = true;
                }
            }
            "Locator" => current.device.locator = non_empty(value),
            "Bank Locator" => current.device.bank_locator = non_empty(value),
            "Type" => current.device.memory_type = non_unknown(value),
            "Speed" => current.device.speed_mts = parse_speed_mts(value),
            "Configured Memory Speed" => {
                current.device.configured_speed_mts = parse_speed_mts(value);
            }
            "Manufacturer" => current.device.manufacturer = non_unknown(value),
            _ => {}
        }
    }

    push_current(&mut devices, &mut current);
    devices
}

pub fn summarize_devices(devices: Vec<MemoryDevice>) -> MemoryDetails {
    let total_installed_bytes = devices.iter().map(|device| device.size_bytes).sum();
    let configured_speed_mts = devices
        .iter()
        .filter_map(|device| device.configured_speed_mts.or(device.speed_mts))
        .max();

    MemoryDetails {
        total_installed_bytes,
        configured_speed_mts,
        devices,
    }
}

pub fn collect() -> (MemoryDetails, Option<String>) {
    match Command::new("dmidecode").args(["--type", "memory"]).output() {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            (
                summarize_devices(parse_dmidecode_memory(&stdout)),
                None,
            )
        }
        Ok(output) => (
            MemoryDetails::default(),
            Some(format!("dmidecode exited with status {}", output.status)),
        ),
        Err(error) => (
            MemoryDetails::default(),
            Some(format!("dmidecode unavailable: {error}")),
        ),
    }
}

fn push_current(devices: &mut Vec<MemoryDevice>, current: &mut DeviceBuilder) {
    if current.installed {
        devices.push(std::mem::take(&mut current.device));
    }
    *current = DeviceBuilder::default();
}

fn parse_size_bytes(value: &str) -> Option<u64> {
    if value.contains("No Module Installed") || value.eq_ignore_ascii_case("unknown") {
        return None;
    }

    let mut parts = value.split_whitespace();
    let amount = parts.next()?.parse::<u64>().ok()?;
    let unit = parts.next().unwrap_or("B").to_ascii_lowercase();
    let multiplier = match unit.as_str() {
        "kb" | "kib" => 1024,
        "mb" | "mib" => 1024_u64.pow(2),
        "gb" | "gib" => 1024_u64.pow(3),
        "tb" | "tib" => 1024_u64.pow(4),
        "b" | "bytes" => 1,
        _ => return None,
    };
    Some(amount.saturating_mul(multiplier))
}

fn parse_speed_mts(value: &str) -> Option<u64> {
    if value.eq_ignore_ascii_case("unknown") {
        return None;
    }
    value.split_whitespace().next()?.parse::<u64>().ok()
}

fn non_empty(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_string())
}

fn non_unknown(value: &str) -> Option<String> {
    (!value.is_empty() && !value.eq_ignore_ascii_case("unknown")).then(|| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const DMIDECODE: &str = "\
# dmidecode 3.5
Handle 0x002D, DMI type 17, 92 bytes
Memory Device
	Size: 32 GB
	Locator: Controller0-ChannelA-DIMM0
	Bank Locator: BANK 0
	Type: DDR5
	Speed: 5600 MT/s
	Manufacturer: Kingston
	Configured Memory Speed: 5600 MT/s

Handle 0x002E, DMI type 17, 92 bytes
Memory Device
	Size: No Module Installed
	Locator: Controller0-ChannelA-DIMM1
	Type: Unknown

Handle 0x002F, DMI type 17, 92 bytes
Memory Device
	Size: 32768 MB
	Locator: Controller0-ChannelB-DIMM0
	Type: DDR5
	Speed: 5600 MT/s
	Manufacturer: Kingston
	Configured Memory Speed: 5200 MT/s
";

    #[test]
    fn parses_installed_memory_devices() {
        let devices = parse_dmidecode_memory(DMIDECODE);

        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].locator.as_deref(), Some("Controller0-ChannelA-DIMM0"));
        assert_eq!(devices[0].size_bytes, 32 * 1024 * 1024 * 1024);
        assert_eq!(devices[0].memory_type.as_deref(), Some("DDR5"));
        assert_eq!(devices[0].speed_mts, Some(5600));
        assert_eq!(devices[0].configured_speed_mts, Some(5600));
        assert_eq!(devices[0].manufacturer.as_deref(), Some("Kingston"));
        assert_eq!(devices[1].configured_speed_mts, Some(5200));
    }

    #[test]
    fn summarizes_installed_memory_details() {
        let details = summarize_devices(parse_dmidecode_memory(DMIDECODE));

        assert_eq!(details.total_installed_bytes, 64 * 1024 * 1024 * 1024);
        assert_eq!(details.configured_speed_mts, Some(5600));
        assert_eq!(details.devices.len(), 2);
    }
}
