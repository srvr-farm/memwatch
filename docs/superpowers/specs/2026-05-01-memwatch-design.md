# Memwatch Design

## Goal

Build a Rust terminal application similar to `~/source/cpuwatch`, but focused on memory. The tool shows installed memory, memory speed, current memory bandwidth, system memory usage, resident memory usage, and the processes using the most resident memory. It can be built, installed, and assigned Linux file capabilities needed for privileged memory details.

## Scope

The first version targets Linux on the current Intel system. It should use the Intel uncore IMC PMU counters exposed under `/sys/bus/event_source/devices` for memory bandwidth. It should degrade cleanly on systems without those counters or without sufficient permissions by showing `N/A` and a concise diagnostic.

This version observes memory state only. It does not tune memory timings, change kernel settings, kill processes, persist historical metrics, or run as a daemon.

## User Interface

The app renders an interactive terminal UI using `ratatui` and `crossterm`, following the same style as `cpuwatch`. It uses the terminal alternate screen and updates structured regions in place.

The main screen contains sections for:

- Memory: total, used, available, free, buffers/cache, swap, dirty pages, and writeback.
- DIMMs: installed memory capacity and configured memory speed where DMI data is available.
- Bandwidth: read, write, and total bandwidth, with per-controller rows when useful.
- Top RSS: process id, name or command, resident memory, and percent of total memory for the highest-RSS processes.
- Diagnostics: permission, hardware, or data-source limitations.

The app exits on `q`, `Esc`, or `Ctrl-C`.

## CLI

The app accepts the same core options as `cpuwatch`:

- `--interval <duration>` uses human-friendly values like `500ms`, `1s`, or `2.5s`.
- `--once` samples once for baseline state, waits one interval for bandwidth deltas, then prints a plain text report.

The default interval is 1 second.

## Data Sources

System memory comes from `/proc/meminfo`. The parser should extract at least `MemTotal`, `MemFree`, `MemAvailable`, `Buffers`, `Cached`, `SReclaimable`, `Shmem`, `SwapTotal`, `SwapFree`, `Dirty`, `Writeback`, `AnonPages`, and `Slab`. Displayed used memory should be derived from total and available memory rather than treating filesystem cache as hard usage.

Memory speed and installed DIMM details come from SMBIOS/DMI memory-device records. The implementation should prefer direct table reads from `/sys/firmware/dmi/tables` when practical, because that avoids depending on output formatting from external commands. If direct SMBIOS parsing is too large for the first implementation, a small parser for `dmidecode --type memory` output is acceptable as an initial backend. Missing or unreadable DMI data should not prevent the app from running.

Memory bandwidth comes from Intel uncore IMC PMU counters. On the current system, the app should discover `uncore_imc_free_running_*` devices and use their exported `data_read`, `data_write`, and `data_total` events. The event `scale` and `unit` files must be honored so raw deltas are converted to MiB/s or GiB/s correctly. The sampler stores previous counter values and elapsed time in memory, then computes bandwidth from deltas.

Top resident processes come from `/proc/<pid>/status`. The scanner should read process name, pid, uid where available, and `VmRSS`. It should sort by RSS descending and keep a bounded list for display. Processes that exit or become unreadable while scanning should be skipped.

## Permissions And Installation

The binary should not require root to show basic `/proc/meminfo` and process RSS data. Installation should support assigning Linux file capabilities for privileged details:

- `cap_perfmon+ep` for perf event access to hardware counters on kernels that support it.
- `cap_dac_read_search+ep` for reading root-owned DMI tables and otherwise restricted details.

The `Makefile` should mirror `cpuwatch` with `build`, `install`, `install-binary`, `capability`, `show-capability`, `uninstall`, `test`, `fmt`, `clippy`, `check`, and `clean` targets. `make install` should build or require the release binary, install it under `$(PREFIX)/bin`, set capabilities, and print the resulting capability set with `getcap`.

If the kernel still blocks perf events, for example because `perf_event_paranoid` is restrictive, the app should report that limitation in diagnostics rather than failing.

## Error Handling

Missing optional data sources produce `N/A` values and diagnostics. Examples include unavailable DMI tables, missing IMC PMUs, perf permission failures, and process entries that disappear during a scan.

Fatal errors should be limited to startup or terminal lifecycle failures that make the app unusable, such as failing to enter raw mode or initialize the terminal backend.

## Code Organization

The crate should use small modules that mirror `cpuwatch` where possible:

- `cli`: command-line parsing and interval configuration.
- `memory`: `/proc/meminfo` parsing and memory summary calculations.
- `dmi`: installed memory and speed discovery.
- `bandwidth`: Intel uncore IMC event discovery, perf counter setup, and bandwidth calculations.
- `processes`: `/proc/<pid>/status` scanning and top-RSS sorting.
- `snapshot`: combined application state and previous-sample tracking.
- `render`: TUI layout and text report formatting.
- `main` and `lib`: app lifecycle, terminal handling, and mode selection.

Hardware-facing code should use injectable paths where practical so parsing and calculations can be tested with fixtures.

## Testing

Implementation should use test-driven development for behavior that can be tested without live hardware:

- Parse duration values for `--interval`.
- Parse `/proc/meminfo` fixtures and compute memory summary values.
- Parse representative DMI or `dmidecode --type memory` records for capacity and speed.
- Discover IMC PMU event metadata from fake sysfs trees.
- Convert raw bandwidth counter deltas into MiB/s and GiB/s.
- Parse `/proc/<pid>/status` fixtures for `Name`, `Pid`, `Uid`, and `VmRSS`.
- Sort and limit top RSS process rows.
- Render a text report in `--once` mode with the expected sections.

Live behavior is verified with `cargo test`, `cargo run -- --once`, and a short interactive TUI launch.
