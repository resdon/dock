# Rclone mount and process tray widgets

This build adds two native status-bar popups.

## Rclone mounts

The storage icon opens a monitor for active rclone FUSE mounts. It reads
`/proc/self/mountinfo`, filters `fuse.rclone` entries, and queries `rclone about
--json` for each remote (cached for 60 seconds). It also reads `statvfs(3)` as a
fallback. The panel shows mount point, remote source, used/total capacity,
percentage, and a compact usage bar. It pages four mounts at a time and refreshes
every 15 seconds while open. The monitor is idle while its popup is hidden.

Some remote backends do not support rclone's About API. Rclone can expose a
synthetic 1 PiB filesystem size for those remotes; the widget recognizes that
placeholder and avoids presenting it as a real quota. If a backend reports
used/free values but no total quota, those values are shown without inventing a
total. This panel is read-only; it does not start, stop, or unmount rclone
processes.

## Processes

The processor icon opens the live process monitor. It uses `sysinfo` on a
background thread, samples about every 1.2 seconds while open, and shows the
process name, PID, CPU percentage, and resident memory. It refreshes only CPU
and memory fields rather than executable paths, command lines, and environment
metadata. The header toggles sorting by CPU or RAM, and the footer pages through
the process list. The monitor stops refreshing when hidden. No process is killed
or signalled by the widget.

## Performance and permissions

Both monitors are demand-driven and keep process/filesystem discovery off the
Wayland render thread. They do not require root privileges. Process memory and
CPU visibility follows the permissions of the dock process and the kernel's
`/proc` access policy. Per-process CPU can exceed 100% when a process uses more
than one logical CPU, matching sysinfo's per-process CPU convention.
