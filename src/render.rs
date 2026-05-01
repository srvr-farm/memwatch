use crate::snapshot::Snapshot;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;
use std::fmt::Write;

pub fn format_text_report(snapshot: &Snapshot) -> String {
    let mut report = String::new();

    writeln!(report, "memory:").unwrap();
    writeln!(report, "  total: {}", format_bytes(snapshot.memory.total_bytes)).unwrap();
    writeln!(
        report,
        "  used: {} ({})",
        format_bytes(snapshot.memory.used_bytes),
        format_percent(snapshot.memory.used_percent)
    )
    .unwrap();
    writeln!(
        report,
        "  available: {}",
        format_bytes(snapshot.memory.available_bytes)
    )
    .unwrap();
    writeln!(report, "  free: {}", format_bytes(snapshot.memory.free_bytes)).unwrap();
    writeln!(
        report,
        "  buffers_cache: {} buffers, {} cache",
        format_bytes(snapshot.memory.buffers_bytes),
        format_bytes(snapshot.memory.cache_bytes)
    )
    .unwrap();
    writeln!(
        report,
        "  swap: {} used / {} total",
        format_bytes(snapshot.memory.swap_used_bytes),
        format_bytes(snapshot.memory.swap_total_bytes)
    )
    .unwrap();
    writeln!(
        report,
        "  dirty_writeback: {} dirty, {} writeback",
        format_bytes(snapshot.memory.dirty_bytes),
        format_bytes(snapshot.memory.writeback_bytes)
    )
    .unwrap();

    writeln!(report, "dimms:").unwrap();
    writeln!(
        report,
        "  installed: {}",
        format_bytes(snapshot.dmi.total_installed_bytes)
    )
    .unwrap();
    writeln!(
        report,
        "  speed: {}",
        format_speed(snapshot.dmi.configured_speed_mts)
    )
    .unwrap();
    for device in &snapshot.dmi.devices {
        writeln!(
            report,
            "  {}: {} {} {}",
            device.locator.as_deref().unwrap_or("unknown"),
            format_bytes(device.size_bytes),
            device.memory_type.as_deref().unwrap_or("unknown"),
            format_speed(device.configured_speed_mts.or(device.speed_mts))
        )
        .unwrap();
    }

    writeln!(report, "bandwidth:").unwrap();
    if let Some(bandwidth) = &snapshot.bandwidth {
        writeln!(
            report,
            "  read: {}, write: {}, total: {}",
            format_rate(bandwidth.read_mib_s),
            format_rate(bandwidth.write_mib_s),
            format_rate(bandwidth.total_mib_s)
        )
        .unwrap();
        for controller in &bandwidth.controllers {
            writeln!(
                report,
                "  {}: read={}, write={}, total={}",
                controller.controller,
                format_rate(controller.read_mib_s),
                format_rate(controller.write_mib_s),
                format_rate(controller.total_mib_s)
            )
            .unwrap();
        }
    } else {
        writeln!(report, "  read: N/A, write: N/A, total: N/A").unwrap();
    }

    writeln!(report, "top_rss:").unwrap();
    for process in &snapshot.processes {
        writeln!(
            report,
            "  {:>6} {:<24} {:>10} {:>7}",
            process.pid,
            process.name,
            format_bytes(process.rss_bytes),
            format_percent(process.rss_percent)
        )
        .unwrap();
    }

    if !snapshot.diagnostics.is_empty() {
        writeln!(report, "diagnostics:").unwrap();
        for diagnostic in &snapshot.diagnostics {
            writeln!(report, "  {diagnostic}").unwrap();
        }
    }

    report
}

pub fn draw(frame: &mut Frame<'_>, snapshot: &Snapshot) {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(if snapshot.diagnostics.is_empty() {
                0
            } else {
                4
            }),
        ])
        .split(frame.area());

    let title = Paragraph::new("memwatch  q/Esc/Ctrl-C to quit")
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .block(Block::default().borders(Borders::BOTTOM));
    frame.render_widget(title, root[0]);

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(root[1]);
    let left = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(10), Constraint::Min(6)])
        .split(columns[0]);
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(8), Constraint::Min(6)])
        .split(columns[1]);

    frame.render_widget(
        Paragraph::new(memory_text(snapshot))
            .block(Block::default().title("Memory").borders(Borders::ALL))
            .wrap(Wrap { trim: false }),
        left[0],
    );
    frame.render_widget(
        Paragraph::new(dimms_text(snapshot))
            .block(Block::default().title("DIMMs").borders(Borders::ALL))
            .wrap(Wrap { trim: false }),
        left[1],
    );
    frame.render_widget(
        Paragraph::new(bandwidth_text(snapshot))
            .block(Block::default().title("Bandwidth").borders(Borders::ALL))
            .wrap(Wrap { trim: false }),
        right[0],
    );
    frame.render_widget(
        Paragraph::new(process_text(snapshot))
            .block(Block::default().title("Top RSS").borders(Borders::ALL))
            .wrap(Wrap { trim: false }),
        right[1],
    );

    if !snapshot.diagnostics.is_empty() {
        frame.render_widget(
            Paragraph::new(snapshot.diagnostics.join("\n"))
                .style(Style::default().fg(Color::Yellow))
                .block(Block::default().title("Diagnostics").borders(Borders::ALL))
                .wrap(Wrap { trim: false }),
            root[2],
        );
    }
}

fn memory_text(snapshot: &Snapshot) -> String {
    let mut text = String::new();
    writeln!(
        text,
        "used:       {:>10}  {}",
        format_bytes(snapshot.memory.used_bytes),
        format_percent(snapshot.memory.used_percent)
    )
    .unwrap();
    writeln!(
        text,
        "available:  {:>10}",
        format_bytes(snapshot.memory.available_bytes)
    )
    .unwrap();
    writeln!(
        text,
        "total:      {:>10}",
        format_bytes(snapshot.memory.total_bytes)
    )
    .unwrap();
    writeln!(
        text,
        "free:       {:>10}",
        format_bytes(snapshot.memory.free_bytes)
    )
    .unwrap();
    writeln!(
        text,
        "cache:      {:>10}",
        format_bytes(snapshot.memory.cache_bytes)
    )
    .unwrap();
    writeln!(
        text,
        "swap:       {:>10} / {}",
        format_bytes(snapshot.memory.swap_used_bytes),
        format_bytes(snapshot.memory.swap_total_bytes)
    )
    .unwrap();
    writeln!(
        text,
        "dirty/wb:   {:>10} / {}",
        format_bytes(snapshot.memory.dirty_bytes),
        format_bytes(snapshot.memory.writeback_bytes)
    )
    .unwrap();
    text
}

fn dimms_text(snapshot: &Snapshot) -> String {
    let mut text = String::new();
    writeln!(
        text,
        "installed: {}",
        format_bytes(snapshot.dmi.total_installed_bytes)
    )
    .unwrap();
    writeln!(
        text,
        "speed:     {}",
        format_speed(snapshot.dmi.configured_speed_mts)
    )
    .unwrap();
    for device in &snapshot.dmi.devices {
        writeln!(
            text,
            "{:<18} {:>8} {:<6} {}",
            device.locator.as_deref().unwrap_or("unknown"),
            format_bytes(device.size_bytes),
            device.memory_type.as_deref().unwrap_or("unknown"),
            format_speed(device.configured_speed_mts.or(device.speed_mts))
        )
        .unwrap();
    }
    text
}

fn bandwidth_text(snapshot: &Snapshot) -> String {
    let mut text = String::new();
    if let Some(bandwidth) = &snapshot.bandwidth {
        writeln!(text, "read:  {}", format_rate(bandwidth.read_mib_s)).unwrap();
        writeln!(text, "write: {}", format_rate(bandwidth.write_mib_s)).unwrap();
        writeln!(text, "total: {}", format_rate(bandwidth.total_mib_s)).unwrap();
        for controller in &bandwidth.controllers {
            writeln!(
                text,
                "{:<26} r={} w={} t={}",
                controller.controller,
                format_rate(controller.read_mib_s),
                format_rate(controller.write_mib_s),
                format_rate(controller.total_mib_s)
            )
            .unwrap();
        }
    } else {
        writeln!(text, "read:  N/A").unwrap();
        writeln!(text, "write: N/A").unwrap();
        writeln!(text, "total: N/A").unwrap();
    }
    text
}

fn process_text(snapshot: &Snapshot) -> String {
    let mut text = String::new();
    writeln!(text, "{:>6} {:<22} {:>10} {:>7}", "pid", "name", "rss", "mem").unwrap();
    for process in &snapshot.processes {
        writeln!(
            text,
            "{:>6} {:<22} {:>10} {:>7}",
            process.pid,
            truncate(&process.name, 22),
            format_bytes(process.rss_bytes),
            format_percent(process.rss_percent)
        )
        .unwrap();
    }
    text
}

fn format_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;
    let bytes = bytes as f64;
    if bytes >= GIB {
        format!("{:.1} GiB", bytes / GIB)
    } else if bytes >= MIB {
        format!("{:.1} MiB", bytes / MIB)
    } else if bytes >= KIB {
        format!("{:.1} KiB", bytes / KIB)
    } else {
        format!("{bytes:.0} B")
    }
}

fn format_rate(value: Option<f64>) -> String {
    value
        .map(|mib_s| {
            if mib_s >= 1024.0 {
                format!("{:.2} GiB/s", mib_s / 1024.0)
            } else {
                format!("{mib_s:.1} MiB/s")
            }
        })
        .unwrap_or_else(|| "N/A".to_string())
}

fn format_percent(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.1}%"))
        .unwrap_or_else(|| "N/A".to_string())
}

fn format_speed(value: Option<u64>) -> String {
    value
        .map(|value| format!("{value} MT/s"))
        .unwrap_or_else(|| "N/A".to_string())
}

fn truncate(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        value.to_string()
    } else {
        value.chars().take(max_chars).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bandwidth::{BandwidthSnapshot, ControllerBandwidth};
    use crate::dmi::{MemoryDetails, MemoryDevice};
    use crate::memory::MemorySummary;
    use crate::processes::ProcessMemory;
    use crate::snapshot::Snapshot;

    #[test]
    fn text_report_contains_all_major_sections() {
        let snapshot = Snapshot {
            memory: MemorySummary {
                total_bytes: 64 * 1024 * 1024 * 1024,
                used_bytes: 16 * 1024 * 1024 * 1024,
                available_bytes: 48 * 1024 * 1024 * 1024,
                free_bytes: 8 * 1024 * 1024 * 1024,
                buffers_bytes: 1 * 1024 * 1024 * 1024,
                cache_bytes: 12 * 1024 * 1024 * 1024,
                swap_total_bytes: 8 * 1024 * 1024 * 1024,
                swap_used_bytes: 0,
                dirty_bytes: 32 * 1024 * 1024,
                writeback_bytes: 0,
                anon_bytes: 10 * 1024 * 1024 * 1024,
                slab_bytes: 2 * 1024 * 1024 * 1024,
                used_percent: Some(25.0),
            },
            dmi: MemoryDetails {
                total_installed_bytes: 64 * 1024 * 1024 * 1024,
                configured_speed_mts: Some(5600),
                devices: vec![MemoryDevice {
                    locator: Some("DIMM0".to_string()),
                    size_bytes: 32 * 1024 * 1024 * 1024,
                    memory_type: Some("DDR5".to_string()),
                    configured_speed_mts: Some(5600),
                    ..MemoryDevice::default()
                }],
            },
            bandwidth: Some(BandwidthSnapshot {
                read_mib_s: Some(1000.0),
                write_mib_s: Some(500.0),
                total_mib_s: Some(1500.0),
                controllers: vec![ControllerBandwidth {
                    controller: "uncore_imc_free_running_0".to_string(),
                    read_mib_s: Some(1000.0),
                    write_mib_s: Some(500.0),
                    total_mib_s: Some(1500.0),
                }],
            }),
            processes: vec![ProcessMemory {
                pid: 42,
                name: "worker".to_string(),
                uid: Some(1000),
                rss_bytes: 2 * 1024 * 1024 * 1024,
                rss_percent: Some(3.125),
            }],
            diagnostics: vec!["perf unavailable".to_string()],
        };

        let report = format_text_report(&snapshot);

        assert!(report.contains("memory:"));
        assert!(report.contains("dimms:"));
        assert!(report.contains("bandwidth:"));
        assert!(report.contains("top_rss:"));
        assert!(report.contains("diagnostics:"));
        assert!(report.contains("worker"));
    }
}
