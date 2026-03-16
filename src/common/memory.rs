use std::process::Command;

#[derive(Debug, Clone, Copy)]
pub struct ProcessMemorySnapshot {
    pub rss_bytes: u64,
    pub vsz_bytes: u64,
}

pub fn sample_process_memory() -> Option<ProcessMemorySnapshot> {
    let pid = std::process::id().to_string();
    let output = Command::new("ps")
        .args(["-o", "rss=", "-o", "vsz=", "-p", &pid])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8(output.stdout).ok()?;
    let mut parts = stdout.split_whitespace();
    let rss_kib = parts.next()?.parse::<u64>().ok()?;
    let vsz_kib = parts.next()?.parse::<u64>().ok()?;

    Some(ProcessMemorySnapshot {
        rss_bytes: rss_kib.saturating_mul(1024),
        vsz_bytes: vsz_kib.saturating_mul(1024),
    })
}

pub fn format_bytes(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = KIB * 1024;
    const GIB: u64 = MIB * 1024;

    if bytes >= GIB {
        format!("{:.2} GiB", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.2} MiB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.2} KiB", bytes as f64 / KIB as f64)
    } else {
        format!("{} B", bytes)
    }
}
