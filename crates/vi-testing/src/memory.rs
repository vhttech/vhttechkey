/// Return the current process's resident set size in kilobytes by reading
/// the `VmRSS:` field from `/proc/self/status`.  Returns 0 if the field
/// cannot be found or parsed (e.g., on non-Linux platforms).
pub fn idle_rss_kb() -> u64 {
    let status = match std::fs::read_to_string("/proc/self/status") {
        Ok(s) => s,
        Err(_) => return 0,
    };
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            if let Some(kb) = rest.split_whitespace().next().and_then(|s| s.parse().ok()) {
                return kb;
            }
        }
    }
    0
}
