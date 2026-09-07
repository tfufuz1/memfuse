use std::fs;

/// Returns current process open file descriptor count.
#[allow(dead_code)]
pub fn get_open_fd_count() -> usize {
    if let Ok(entries) = fs::read_dir("/proc/self/fd") {
        entries.filter_map(|e| e.ok()).count()
    } else {
        0
    }
}

/// Returns current process Resident Set Size (RSS) in bytes.
#[allow(dead_code)]
pub fn get_rss_bytes() -> usize {
    if let Ok(statm) = fs::read_to_string("/proc/self/statm") {
        let parts: Vec<&str> = statm.split_whitespace().collect();
        if parts.len() >= 2 {
            if let Ok(pages) = parts[1].parse::<usize>() {
                return pages * 4096; // 4KB page size
            }
        }
    }
    0
}
