# memwatch - Linux Memory Monitor and Bandwidth TUI

`memwatch` is a read-only Linux memory monitor and terminal TUI for RAM usage,
installed DIMM details, memory bandwidth, and the processes using the most
resident memory. It is useful when you want a lightweight Rust alternative or
companion to tools like `free`, `top`, `htop`, `btop`, `vmstat`, `dmidecode`,
and `perf`.

The default mode is an interactive terminal UI. A `--once` mode is also
available for scripts, diagnostics, CI logs, and non-interactive environments.

## What You Can Monitor

Use `memwatch` when you want to:

- Monitor Linux memory usage from a terminal, including used, available, free,
  cache, buffers, swap, dirty pages, and writeback.
- Find the highest-RSS processes without opening a full process manager.
- Inspect installed RAM, DIMM slots, memory type, manufacturer, and configured
  memory speed from SMBIOS/DMI data.
- Measure memory bandwidth with Linux perf PMU counters when the hardware and
  kernel expose usable events.
- Watch Intel uncore IMC memory-controller read/write bandwidth in MiB/s or
  GiB/s.
- Debug laptop, desktop, workstation, homelab, and server memory behavior
  without changing kernel settings or killing processes.

## Features

- Shows total, used, available, free, buffers/cache, swap, dirty, and writeback
  memory from `/proc/meminfo`.
- Shows installed memory capacity, DIMM locator, memory type, manufacturer, and
  configured speed when SMBIOS/DMI data is readable.
- Measures memory bandwidth from Linux perf PMU counters when supported by the
  kernel and hardware.
- Shows aggregate and per-controller read, write, and total bandwidth when those
  PMU events are available.
- Lists the highest-RSS processes by scanning `/proc/<pid>/status`.
- Provides interactive TUI and one-shot plain-text report modes.
- Degrades to `N/A` values and diagnostics when optional data sources are
  missing, restricted, or unsupported.

## Keywords

Linux memory monitor, terminal memory monitor, Linux RAM monitor, Rust TUI
memory monitor, memory bandwidth monitor, RAM bandwidth monitor, Intel uncore
IMC monitor, Linux perf PMU, `perf_event_open`, top RSS processes, process memory
usage, DIMM information, memory speed monitor, SMBIOS memory, DMI memory,
`dmidecode`, `/proc/meminfo`, `/proc` process monitor.

## Quick Start

Run from the repository without installing:

```sh
cargo run -- --once
cargo run -- --interval 500ms
```

Build and install the release binary:

```sh
make install
memwatch --once
memwatch
```

If `/usr/local/bin` is not in your `PATH`, either add it or install with a custom
`PREFIX`, `BINDIR`, or `INSTALL_PATH`.

## Supported Systems

`memwatch` targets Linux systems that expose memory and process data through
procfs, SMBIOS/DMI data through sysfs or `dmidecode`, and bandwidth events
through Linux perf PMUs.

| System type | Support level | Notes |
| --- | --- | --- |
| Bare-metal Linux on Intel x86/x86_64 with uncore IMC PMUs | Full | Expected to show memory usage, DIMM details, top RSS processes, and read/write/total memory bandwidth when permissions are set up. |
| Linux on Intel x86/x86_64 without IMC PMUs | Partial | Memory usage, DIMM details, and top RSS processes may work. Bandwidth shows `N/A` with a diagnostic. |
| Linux on AMD x86/x86_64 | Partial | Memory usage, DIMM details, and top RSS processes may work. Bandwidth support is limited to available CPU PMU fallback events and may only report read-side activity. |
| Linux VMs, containers, or restricted hosts | Partial | `/proc/meminfo` usually works, but DMI tables and hardware PMU counters are often hidden or blocked. |
| Non-x86 Linux | Partial | Basic `/proc` memory and process data may work. DIMM and bandwidth data depend on platform firmware and PMU support. |
| macOS, Windows, BSD, WSL without Linux hardware sysfs/perf access | Not supported for useful runtime data | The crate may compile on some non-Linux targets, but the monitor expects Linux `/proc`, `/sys`, and perf hardware-counter interfaces. |

The TUI requires an interactive terminal. Use `--once` for automation or
non-interactive environments.

## Data Sources

| Data | Source |
| --- | --- |
| System memory summary | `/proc/meminfo` |
| Top RSS processes | `/proc/<pid>/status` |
| Direct DMI/SMBIOS table | `/sys/firmware/dmi/tables/DMI` |
| DMI fallback | `dmidecode --type memory` |
| PMU event metadata | `/sys/bus/event_source/devices` |
| CPU online list for AMD fallback PMU events | `/sys/devices/system/cpu/online` |
| Memory bandwidth counters | `perf_event_open` |

`memwatch` does not change memory timings, kernel tunables, process state, swap
settings, or any other system configuration.

## Prerequisites

Required for building:

- A current stable Rust toolchain.
- Cargo.

The manifest does not currently declare a minimum supported Rust version. Use the
repository's `Cargo.lock` for reproducible dependency versions.

Optional but recommended:

- `make`, for the repository build/install targets.
- `sudo`, `setcap`, and `getcap`, for installing the binary with Linux file
  capabilities.
- `dmidecode`, as a fallback when direct DMI table reads are unavailable or
  return no installed memory-device records.

On Debian or Ubuntu-style systems, the optional runtime tools are typically in:

```sh
sudo apt install make libcap2-bin dmidecode
```

On Fedora-style systems:

```sh
sudo dnf install make libcap dmidecode
```

Distribution package names vary. If `setcap`, `getcap`, or `dmidecode` are not
in `PATH`, install the package that provides them for your distribution.

## Building

Build a debug binary:

```sh
cargo build
```

Build an optimized release binary:

```sh
cargo build --release
```

The release binary is written to:

```sh
target/release/memwatch
```

The Makefile wraps the release build:

```sh
make build
```

## Building Packages

Build Debian and RPM packages:

```sh
make package VERSION=0.1.12
make check-packages VERSION=0.1.12
```

Package artifacts are written to `dist/` by default:

- `memwatch_0.1.12_amd64.deb`
- `memwatch-0.1.12-1.x86_64.rpm`

Both packages install `memwatch` to `/usr/bin/memwatch`, keep the binary
executable, and run this during package installation:

```sh
setcap cap_perfmon,cap_dac_read_search+ep /usr/bin/memwatch
```

Required package build tools:

- `dpkg-deb`, usually provided by the Debian or Ubuntu `dpkg` package.
- `rpmbuild`, usually provided by the Fedora, RHEL, or Debian `rpm` package.

## Development Checks

Run the full local check suite:

```sh
make check
```

That runs:

```sh
cargo fmt --check
cargo test
cargo clippy -- -D warnings
```

You can also run individual targets:

```sh
make fmt
make test
make clippy
```

## Installing

The recommended install path is through the Makefile:

```sh
make install
```

By default this:

1. Builds `target/release/memwatch` if needed.
2. Installs it to `/usr/local/bin/memwatch`.
3. Applies the `cap_perfmon,cap_dac_read_search+ep` file capability set.
4. Prints the resulting capability with `getcap`.

Verify the installed command:

```sh
command -v memwatch
memwatch --once
```

If you prefer to run the privileged install step explicitly, build first and
then run install under `sudo`:

```sh
make build
sudo make install
```

The prebuild matters because `sudo make install` runs as root and the Makefile
expects the release binary to already exist in that case.

### Custom Install Paths

Install under a different prefix:

```sh
PREFIX="$HOME/.local" make install
```

Install to a specific binary directory:

```sh
BINDIR="$HOME/.local/bin" make install
```

Install to an exact path:

```sh
INSTALL_PATH="$HOME/.local/bin/memwatch" make install
```

### Installing Without Capabilities

To install only the binary:

```sh
make install-binary
```

Without capabilities, `memwatch` still runs, but DMI details and memory
bandwidth counters may be unavailable.

You can apply or reapply capabilities later:

```sh
make capability
```

Check the installed capabilities:

```sh
make show-capability
getcap "$(command -v memwatch)"
```

Remove the installed binary:

```sh
make uninstall
```

### Cargo Install

You can also install with Cargo:

```sh
cargo install --path .
```

Cargo does not apply Linux file capabilities. If you need protected DMI table
reads or perf counter access, apply the capabilities manually or use
`make install`.

## Runtime Setup

### Basic Memory And Processes

The memory summary and top-RSS process list rely on `/proc`:

```sh
test -r /proc/meminfo
ls /proc/1/status
```

These data sources normally work as an unprivileged user on Linux. Some
containers or hardened hosts may hide process details for other users.

### DIMM And Speed Details

`memwatch` first tries to read SMBIOS/DMI memory-device records directly from:

```sh
/sys/firmware/dmi/tables/DMI
```

If that does not return installed memory-device records, it falls back to:

```sh
dmidecode --type memory
```

Direct DMI table reads and `dmidecode` commonly require elevated privileges or
the `cap_dac_read_search+ep` file capability. If DIMM details show as `N/A`, use
the Makefile install path or check the source manually:

```sh
sudo dmidecode --type memory
```

### Memory Bandwidth

Bandwidth readings use Linux perf hardware counters. `memwatch` discovers events
under:

```sh
/sys/bus/event_source/devices
```

On Intel systems, full bandwidth support expects `uncore_imc_*` or
`uncore_imc_free_running_*` event-source devices with exported read and write
events. Check for them with:

```sh
ls /sys/bus/event_source/devices | grep uncore_imc
```

The binary opens PMU counters with `perf_event_open`. The Makefile applies:

```sh
cap_perfmon,cap_dac_read_search+ep
```

`cap_perfmon` is the relevant capability for perf counter access on kernels that
support it. Even with this capability, the kernel may still block access through
perf policy. Check the current policy:

```sh
cat /proc/sys/kernel/perf_event_paranoid
```

If bandwidth remains `N/A`, run a one-off diagnostic as root:

```sh
sudo target/release/memwatch --once
```

Some systems do not expose memory-controller PMU events at all. In that case the
rest of the monitor still works and the bandwidth panel reports `N/A`.

### Capabilities

Some filesystems, package managers, or copy operations do not preserve Linux file
capabilities. If the installed binary is replaced after install, run:

```sh
make capability
```

If `setcap` rejects `cap_perfmon`, your kernel or libcap tooling may be too old
for that capability name. Basic memory reporting still works without it, but
hardware bandwidth counters will likely remain unavailable.

## Usage

Start the interactive TUI:

```sh
memwatch
```

Exit the TUI with any of:

- `q`
- `Esc`
- `Ctrl-C`

Use a custom update interval:

```sh
memwatch --interval 500ms
memwatch --interval 2s
```

Print one text report and exit:

```sh
memwatch --once
```

Use a custom sampling interval for the one-shot report:

```sh
memwatch --once --interval 250ms
```

In `--once` mode, `memwatch` takes an initial sample, waits for the interval,
then takes a second sample so bandwidth can be computed from counter deltas.

Show CLI help:

```sh
memwatch --help
```

Current options:

```text
Usage: memwatch [OPTIONS]

Options:
      --interval <INTERVAL>  [default: 1s]
      --once
  -h, --help                 Print help
```

## Troubleshooting

- `meminfo unavailable or unreadable`: the process cannot read `/proc/meminfo`.
  This usually means the program is running outside Linux or inside an unusually
  restricted environment.
- `DMI table unreadable`: direct SMBIOS/DMI table reads failed. Install with
  `make install`, check capabilities with `getcap "$(command -v memwatch)"`, or
  verify `sudo dmidecode --type memory`.
- `dmidecode unavailable`: install `dmidecode` if you want the fallback path for
  DIMM details.
- `no memory bandwidth PMU events found`: the kernel did not expose supported
  PMU events. Basic memory and process reporting can still work.
- `failed to open ...`: PMU events were discovered, but `perf_event_open` was
  blocked or failed. Check capabilities and `/proc/sys/kernel/perf_event_paranoid`.
- Bandwidth shows `N/A` on the first update: bandwidth requires two counter
  samples. Wait for the next interval, or use `--once` with a nonzero interval.
- Top RSS is empty: `/proc` process status files may be hidden or inaccessible in
  the current environment.
- The TUI does not render correctly: use a real interactive terminal with enough
  width and height, or use `memwatch --once` for plain text output.

## Repository Layout

- `src/main.rs`: binary entry point.
- `src/lib.rs`: mode selection, TUI loop, and terminal lifecycle.
- `src/cli.rs`: command-line options.
- `src/memory.rs`: `/proc/meminfo` parsing and memory summary calculations.
- `src/dmi.rs`: direct SMBIOS/DMI parsing and `dmidecode` fallback parsing.
- `src/bandwidth.rs`: PMU event discovery, perf counter setup, and bandwidth
  calculations.
- `src/processes.rs`: `/proc/<pid>/status` scanning and top-RSS sorting.
- `src/snapshot.rs`: combined sampling state.
- `src/render.rs`: TUI rendering and one-shot text reports.
- `Makefile`: build, install, capability, and check targets.

## Suggested GitHub Topics

For the repository About sidebar, useful topics would be:

```text
linux
rust
tui
terminal
memory-monitor
ram-monitor
hardware-monitor
performance-monitoring
memory-bandwidth
perf
pmu
dmidecode
ratatui
```
