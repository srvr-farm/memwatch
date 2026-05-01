use std::fs;
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
    let direct_error = match fs::read("/sys/firmware/dmi/tables/DMI") {
        Ok(table) => {
            let devices = parse_dmi_table(&table);
            if !devices.is_empty() {
                return (summarize_devices(devices), None);
            }
            Some("DMI table contained no installed memory-device records".to_string())
        }
        Err(error) => Some(format!("DMI table unreadable: {error}")),
    };

    match Command::new("dmidecode")
        .args(["--type", "memory"])
        .output()
    {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            (summarize_devices(parse_dmidecode_memory(&stdout)), None)
        }
        Ok(output) => (
            MemoryDetails::default(),
            Some(format!(
                "{}; dmidecode exited with status {}",
                direct_error.unwrap_or_else(|| "DMI table unavailable".to_string()),
                output.status
            )),
        ),
        Err(error) => (
            MemoryDetails::default(),
            Some(format!(
                "{}; dmidecode unavailable: {error}",
                direct_error.unwrap_or_else(|| "DMI table unavailable".to_string())
            )),
        ),
    }
}

pub fn parse_dmi_table(table: &[u8]) -> Vec<MemoryDevice> {
    let mut devices = Vec::new();
    let mut offset = 0;

    while offset + 4 <= table.len() {
        let structure_type = table[offset];
        let length = table[offset + 1] as usize;
        if length < 4 || offset + length > table.len() {
            break;
        }

        let strings_start = offset + length;
        let Some((strings, next_offset)) = parse_structure_strings(table, strings_start) else {
            break;
        };

        if structure_type == 17 {
            if let Some(device) =
                parse_memory_device_record(&table[offset..offset + length], &strings)
            {
                devices.push(device);
            }
        }

        offset = next_offset;
    }

    devices
}

fn push_current(devices: &mut Vec<MemoryDevice>, current: &mut DeviceBuilder) {
    if current.installed {
        devices.push(std::mem::take(&mut current.device));
    }
    *current = DeviceBuilder::default();
}

fn parse_structure_strings(table: &[u8], start: usize) -> Option<(Vec<String>, usize)> {
    if start >= table.len() {
        return None;
    }

    let mut end = start;
    while end + 1 < table.len() {
        if table[end] == 0 && table[end + 1] == 0 {
            let strings = table[start..end]
                .split(|byte| *byte == 0)
                .filter(|value| !value.is_empty())
                .map(|value| String::from_utf8_lossy(value).to_string())
                .collect::<Vec<_>>();
            return Some((strings, end + 2));
        }
        end += 1;
    }

    None
}

fn parse_memory_device_record(record: &[u8], strings: &[String]) -> Option<MemoryDevice> {
    let size_bytes = smbios_size_bytes(record)?;
    Some(MemoryDevice {
        locator: smbios_string(strings, byte_at(record, 0x10)?),
        bank_locator: smbios_string(strings, byte_at(record, 0x11)?),
        size_bytes,
        memory_type: smbios_memory_type(byte_at(record, 0x12)?),
        speed_mts: smbios_word(record, 0x15).and_then(nonzero_word),
        configured_speed_mts: smbios_word(record, 0x20).and_then(nonzero_word),
        manufacturer: smbios_string(strings, byte_at(record, 0x17)?)
            .and_then(|value| non_unknown(&value)),
    })
}

fn smbios_size_bytes(record: &[u8]) -> Option<u64> {
    let size = smbios_word(record, 0x0c)?;
    match size {
        0 | 0xffff => None,
        0x7fff => {
            let extended_size_mb = smbios_dword(record, 0x1c)?;
            (extended_size_mb > 0).then_some(extended_size_mb.saturating_mul(1024 * 1024))
        }
        value if value & 0x8000 != 0 => Some(((value & 0x7fff) as u64).saturating_mul(1024)),
        value => Some((value as u64).saturating_mul(1024 * 1024)),
    }
}

fn smbios_string(strings: &[String], index: u8) -> Option<String> {
    if index == 0 {
        return None;
    }
    strings
        .get(index as usize - 1)
        .and_then(|value| non_unknown(value))
}

fn smbios_memory_type(value: u8) -> Option<String> {
    let label = match value {
        0x01 => "Other",
        0x02 => "Unknown",
        0x03 => "DRAM",
        0x0f => "SDRAM",
        0x12 => "DDR",
        0x13 => "DDR2",
        0x18 => "DDR3",
        0x1a => "DDR4",
        0x1b => "LPDDR",
        0x1c => "LPDDR2",
        0x1d => "LPDDR3",
        0x1e => "LPDDR4",
        0x22 => "DDR5",
        0x23 => "LPDDR5",
        _ => return None,
    };
    non_unknown(label)
}

fn byte_at(record: &[u8], offset: usize) -> Option<u8> {
    record.get(offset).copied()
}

fn smbios_word(record: &[u8], offset: usize) -> Option<u16> {
    let bytes = record.get(offset..offset + 2)?;
    Some(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn smbios_dword(record: &[u8], offset: usize) -> Option<u64> {
    let bytes = record.get(offset..offset + 4)?;
    Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as u64)
}

fn nonzero_word(value: u16) -> Option<u64> {
    (!matches!(value, 0 | 0xffff)).then_some(value as u64)
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
        assert_eq!(
            devices[0].locator.as_deref(),
            Some("Controller0-ChannelA-DIMM0")
        );
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

    #[test]
    fn parses_smbios_memory_device_records() {
        let mut table = Vec::new();
        table.extend(type17_record(
            0x002d,
            32_768,
            0x22,
            5600,
            5600,
            &["DIMM0", "BANK 0", "Kingston"],
        ));
        table.extend(type17_record(
            0x002e,
            0,
            0x02,
            0,
            0,
            &["DIMM1", "BANK 1", "Unknown"],
        ));

        let devices = parse_dmi_table(&table);

        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].locator.as_deref(), Some("DIMM0"));
        assert_eq!(devices[0].bank_locator.as_deref(), Some("BANK 0"));
        assert_eq!(devices[0].size_bytes, 32 * 1024 * 1024 * 1024);
        assert_eq!(devices[0].memory_type.as_deref(), Some("DDR5"));
        assert_eq!(devices[0].speed_mts, Some(5600));
        assert_eq!(devices[0].configured_speed_mts, Some(5600));
        assert_eq!(devices[0].manufacturer.as_deref(), Some("Kingston"));
    }

    fn type17_record(
        handle: u16,
        size_mb: u32,
        memory_type: u8,
        speed_mts: u16,
        configured_speed_mts: u16,
        strings: &[&str],
    ) -> Vec<u8> {
        let mut record = vec![0_u8; 0x22];
        record[0] = 17;
        record[1] = 0x22;
        record[2..4].copy_from_slice(&handle.to_le_bytes());
        if size_mb > 0x7ffe {
            record[0x0c..0x0e].copy_from_slice(&0x7fff_u16.to_le_bytes());
            record[0x1c..0x20].copy_from_slice(&size_mb.to_le_bytes());
        } else {
            record[0x0c..0x0e].copy_from_slice(&(size_mb as u16).to_le_bytes());
        }
        record[0x10] = 1;
        record[0x11] = 2;
        record[0x12] = memory_type;
        record[0x15..0x17].copy_from_slice(&speed_mts.to_le_bytes());
        record[0x17] = 3;
        record[0x20..0x22].copy_from_slice(&configured_speed_mts.to_le_bytes());
        for value in strings {
            record.extend(value.as_bytes());
            record.push(0);
        }
        record.push(0);
        record
    }
}
