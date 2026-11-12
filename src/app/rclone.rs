//! Rclone monitor service.
//!
//! Polls rclone mount status and transfer rates for the storage panel.

use std::collections::HashMap;
use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use crate::models::{RcloneMount, RcloneSnapshot};

/// Monitor rclone FUSE mounts only while the tray panel is open. Reading
/// mountinfo/statvfs is cheap, but there is no reason to poll while hidden.
pub fn spawn_rclone_monitor(snapshot_tx: Sender<RcloneSnapshot>, enabled_rx: Receiver<bool>) {
    thread::Builder::new()
        .name("dock-rclone-monitor".into())
        .spawn(move || {
            loop {
                match enabled_rx.recv() {
                    Ok(true) => {}
                    Ok(false) => continue,
                    Err(_) => return,
                }

                let mut enabled = true;
                let mut first_snapshot = true;
                let mut about_cache: HashMap<String, (Instant, Option<(u64, u64, u64)>)> =
                    HashMap::new();
                while enabled {
                    let mut snapshot = read_rclone_snapshot();
                    // Publish mount names immediately, before any remote quota
                    // query. A slow/unreachable cloud backend must not delay the
                    // initial popup contents.
                    let is_first_snapshot = first_snapshot;
                    if is_first_snapshot && snapshot_tx.send(snapshot.clone()).is_err() {
                        return;
                    }
                    first_snapshot = false;
                    let before_enrichment = snapshot.clone();
                    enrich_rclone_capacities(&mut snapshot, &mut about_cache);
                    if (!is_first_snapshot || snapshot != before_enrichment)
                        && snapshot_tx.send(snapshot).is_err()
                    {
                        return;
                    }
                    match enabled_rx.recv_timeout(Duration::from_secs(15)) {
                        Ok(value) => enabled = value,
                        Err(RecvTimeoutError::Timeout) => {}
                        Err(RecvTimeoutError::Disconnected) => return,
                    }
                }
            }
        })
        .ok();
}

const ABOUT_CACHE_TTL: Duration = Duration::from_secs(60);
const ABOUT_COMMAND_TIMEOUT: Duration = Duration::from_secs(8);
const RCLONE_UNLIMITED_FALLBACK: u64 = 1u64 << 50; // rclone's documented 1 PiB fallback

fn enrich_rclone_capacities(
    snapshot: &mut RcloneSnapshot,
    cache: &mut HashMap<String, (Instant, Option<(u64, u64, u64)>)>,
) {
    cache.retain(|_, (updated, _)| updated.elapsed() < ABOUT_CACHE_TTL);
    for mount in &mut snapshot.mounts {
        if mount.source.is_empty() {
            continue;
        }

        let cached = cache
            .get(&mount.source)
            .filter(|(updated, _)| updated.elapsed() < ABOUT_CACHE_TTL);
        let about = if let Some((_, result)) = cached {
            *result
        } else {
            let result = query_rclone_about(&mount.source);
            cache.insert(mount.source.clone(), (Instant::now(), result));
            result
        };

        if let Some((total, used, available)) = about {
            mount.total_bytes = total;
            mount.used_bytes = used;
            mount.available_bytes = available;
        } else if mount.total_bytes == RCLONE_UNLIMITED_FALLBACK && mount.used_bytes == 0 {
            // When a backend has no About API, rclone can report 1 PiB total
            // and 1 PiB free. That is a placeholder, not a real quota.
            mount.total_bytes = 0;
            mount.available_bytes = 0;
        }
    }
}

fn query_rclone_about(remote: &str) -> Option<(u64, u64, u64)> {
    let mut child = Command::new("rclone")
        .arg("about")
        .arg("--json")
        .arg(remote)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let output = child.wait_with_output().ok()?;
                if !status.success() {
                    return None;
                }
                return parse_about_capacity(&output.stdout);
            }
            Ok(None) if started.elapsed() < ABOUT_COMMAND_TIMEOUT => {
                thread::sleep(Duration::from_millis(100));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

fn parse_about_capacity(json: &[u8]) -> Option<(u64, u64, u64)> {
    let value: serde_json::Value = serde_json::from_slice(json).ok()?;
    let reported_used = value.get("used").and_then(serde_json::Value::as_u64);
    let reported_free = value.get("free").and_then(serde_json::Value::as_u64);
    let reported_total = value.get("total").and_then(serde_json::Value::as_u64);

    let total = reported_total
        .or_else(|| {
            reported_used
                .zip(reported_free)
                .map(|(used, free)| used.saturating_add(free))
        })
        .unwrap_or(0);
    let used = reported_used
        .or_else(|| (total > 0).then(|| total.saturating_sub(reported_free.unwrap_or(0))))
        .unwrap_or(0);
    let available = reported_free
        .or_else(|| (total > 0).then(|| total.saturating_sub(used)))
        .unwrap_or(0);

    (total > 0 || used > 0 || available > 0).then_some((total, used, available))
}

fn read_rclone_snapshot() -> RcloneSnapshot {
    match std::fs::read_to_string("/proc/self/mountinfo") {
        Ok(contents) => {
            let mounts = parse_rclone_mountinfo(&contents);
            RcloneSnapshot {
                mounts,
                error: None,
            }
        }
        Err(error) => RcloneSnapshot {
            mounts: Vec::new(),
            error: Some(format!("Cannot read mount table: {error}")),
        },
    }
}

fn decode_mount_field(value: &str) -> String {
    let mut decoded = Vec::with_capacity(value.len());
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\\'
            && index + 3 < bytes.len()
            && bytes[index + 1..index + 4]
                .iter()
                .all(|byte| (b'0'..=b'7').contains(byte))
        {
            let octal = ((bytes[index + 1] - b'0') as u16) * 64
                + ((bytes[index + 2] - b'0') as u16) * 8
                + (bytes[index + 3] - b'0') as u16;
            decoded.push(octal as u8);
            index += 4;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

fn parse_rclone_mountinfo(contents: &str) -> Vec<RcloneMount> {
    let mut mounts = Vec::new();
    for line in contents.lines() {
        let Some((before_separator, after_separator)) = line.split_once(" - ") else {
            continue;
        };
        let before: Vec<&str> = before_separator.split_whitespace().collect();
        let after: Vec<&str> = after_separator.split_whitespace().collect();
        if before.len() < 5 || after.len() < 2 {
            continue;
        }
        let fs_type = after[0];
        if !(fs_type == "fuse.rclone" || fs_type.starts_with("fuse.rclone.")) {
            continue;
        }

        let mount_point = decode_mount_field(before[4]);
        let source = decode_mount_field(after[1]);
        let (total_bytes, used_bytes, available_bytes) =
            filesystem_capacity(Path::new(&mount_point));
        mounts.push(RcloneMount {
            mount_point,
            source,
            fs_type: fs_type.to_string(),
            total_bytes,
            used_bytes,
            available_bytes,
        });
    }
    mounts.sort_by(|a, b| {
        a.mount_point
            .to_lowercase()
            .cmp(&b.mount_point.to_lowercase())
    });
    mounts
}

fn filesystem_capacity(path: &Path) -> (u64, u64, u64) {
    let Ok(path) = CString::new(path.as_os_str().as_bytes()) else {
        return (0, 0, 0);
    };
    // statvfs exposes the capacity reported by this exact FUSE mount. We query
    // `rclone about` separately for authoritative remote quota data and use
    // this as a fast fallback when the CLI/API lookup is unavailable.
    let mut stats = std::mem::MaybeUninit::<libc::statvfs>::zeroed();
    let result = unsafe { libc::statvfs(path.as_ptr(), stats.as_mut_ptr()) };
    if result != 0 {
        return (0, 0, 0);
    }
    let stats = unsafe { stats.assume_init() };
    let block_size = if stats.f_frsize > 0 {
        stats.f_frsize
    } else {
        stats.f_bsize
    };
    let total = stats.f_blocks.saturating_mul(block_size);
    let free = stats.f_bfree.saturating_mul(block_size);
    let available = stats.f_bavail.saturating_mul(block_size);
    let used = total.saturating_sub(free);
    if total == RCLONE_UNLIMITED_FALLBACK && used == 0 {
        // Rclone reports this sentinel when a backend has no quota API; it is
        // not a meaningful total/free-space figure for a capacity widget.
        return (0, 0, 0);
    }
    (total, used, available)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_only_rclone_mounts_and_unescapes_mountpoint() {
        let sample = concat!(
            "36 25 0:32 / /media/cloud\\040drive rw,nosuid - fuse.rclone remote: rw,user_id=1000\n",
            "37 25 8:1 / / rw,relatime - ext4 /dev/sda1 rw\n",
            "38 25 0:33 / /mnt/other rw - fuse.sshfs user@host:/ rw\n",
        );
        let mounts = parse_rclone_mountinfo(sample);
        assert_eq!(mounts.len(), 1);
        assert_eq!(mounts[0].mount_point, "/media/cloud drive");
        assert_eq!(mounts[0].source, "remote:");
        assert_eq!(mounts[0].fs_type, "fuse.rclone");
    }

    #[test]
    fn parses_rclone_about_json_capacity() {
        let json = br#"{"total":1000,"used":400,"free":600}"#;
        assert_eq!(parse_about_capacity(json), Some((1000, 400, 600)));
    }

    #[test]
    fn derives_total_from_used_and_free_when_total_is_omitted() {
        let json = br#"{"used":400,"free":600}"#;
        assert_eq!(parse_about_capacity(json), Some((1000, 400, 600)));
    }

    #[test]
    fn derives_used_from_total_and_free_when_backend_omits_used() {
        let json = br#"{"total":1000,"free":600}"#;
        assert_eq!(parse_about_capacity(json), Some((1000, 400, 600)));
    }

    #[test]
    fn empty_mount_table_returns_empty_list() {
        assert!(parse_rclone_mountinfo("").is_empty());
    }
}
