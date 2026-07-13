//! Best-effort per-process resource sampling for the `:status` line.
//!
//! CPU and memory are read through the platform's native API (libc on Unix,
//! kernel32 on Windows) — no extra dependencies. Per-process GPU utilisation
//! has no portable API: Linux exposes a device-wide figure via sysfs; macOS and
//! Windows have none without private APIs / elevation, so GPU reads as `None`
//! there and renders as `—`.

use std::time::Instant;

use xei_core::ProcMetrics;

pub struct Sampler {
    /// (wall clock, cumulative process CPU seconds) at the previous sample.
    last: Option<(Instant, f64)>,
    total_mem: u64,
}

impl Sampler {
    pub fn new() -> Self {
        Self {
            last: None,
            total_mem: total_physical_memory().max(1),
        }
    }

    /// Take a sample. CPU% needs two readings, so the very first call reports
    /// 0% CPU (memory/GPU are correct immediately).
    pub fn sample(&mut self) -> ProcMetrics {
        let now = Instant::now();
        let cpu_secs = process_cpu_seconds();
        let cpu_pct = match self.last {
            Some((t0, c0)) => {
                let dt = now.duration_since(t0).as_secs_f64();
                if dt > 0.0 {
                    (((cpu_secs - c0) / dt) * 100.0).max(0.0) as f32
                } else {
                    0.0
                }
            }
            None => 0.0,
        };
        self.last = Some((now, cpu_secs));

        let mem_bytes = process_resident_bytes();
        let mem_pct = (mem_bytes as f64 / self.total_mem as f64 * 100.0) as f32;
        let mem_mb = (mem_bytes as f64 / 1_048_576.0) as f32;

        ProcMetrics {
            cpu_pct,
            mem_pct,
            mem_mb,
            gpu_pct: gpu_percent(),
            sampled: true,
        }
    }
}

// ── CPU time ───────────────────────────────────────────────────────────────

#[cfg(unix)]
fn process_cpu_seconds() -> f64 {
    unsafe {
        let mut u: libc::rusage = std::mem::zeroed();
        if libc::getrusage(libc::RUSAGE_SELF, &mut u) == 0 {
            tv_secs(u.ru_utime) + tv_secs(u.ru_stime)
        } else {
            0.0
        }
    }
}

#[cfg(unix)]
fn tv_secs(t: libc::timeval) -> f64 {
    t.tv_sec as f64 + t.tv_usec as f64 / 1_000_000.0
}

#[cfg(windows)]
fn process_cpu_seconds() -> f64 {
    unsafe {
        let mut creation = win::Filetime::default();
        let mut exit = win::Filetime::default();
        let mut kernel = win::Filetime::default();
        let mut user = win::Filetime::default();
        if win::GetProcessTimes(
            win::GetCurrentProcess(),
            &mut creation,
            &mut exit,
            &mut kernel,
            &mut user,
        ) != 0
        {
            // Kernel + user time, each in 100-nanosecond ticks.
            (kernel.as_ticks() + user.as_ticks()) as f64 * 1e-7
        } else {
            0.0
        }
    }
}

// ── Resident memory (current) ──────────────────────────────────────────────

#[cfg(target_os = "macos")]
fn process_resident_bytes() -> u64 {
    unsafe {
        let mut info: libc::rusage_info_v2 = std::mem::zeroed();
        let rc = libc::proc_pid_rusage(
            libc::getpid(),
            libc::RUSAGE_INFO_V2,
            &mut info as *mut libc::rusage_info_v2 as *mut libc::rusage_info_t,
        );
        if rc == 0 {
            // `ri_phys_footprint` matches Activity Monitor's "Memory" column.
            if info.ri_phys_footprint > 0 {
                info.ri_phys_footprint
            } else {
                info.ri_resident_size
            }
        } else {
            0
        }
    }
}

#[cfg(target_os = "linux")]
fn process_resident_bytes() -> u64 {
    // /proc/self/statm: total resident shared ... (all in pages)
    if let Ok(s) = std::fs::read_to_string("/proc/self/statm") {
        let mut it = s.split_whitespace();
        let _total = it.next();
        if let Some(res) = it.next().and_then(|v| v.parse::<u64>().ok()) {
            let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
            if page > 0 {
                return res * page as u64;
            }
        }
    }
    0
}

#[cfg(all(unix, not(any(target_os = "macos", target_os = "linux"))))]
fn process_resident_bytes() -> u64 {
    // Other Unix: ru_maxrss is max (not current) RSS — a coarse fallback. Units
    // are KB on the BSDs this branch covers.
    unsafe {
        let mut u: libc::rusage = std::mem::zeroed();
        if libc::getrusage(libc::RUSAGE_SELF, &mut u) == 0 {
            (u.ru_maxrss as u64).saturating_mul(1024)
        } else {
            0
        }
    }
}

#[cfg(windows)]
fn process_resident_bytes() -> u64 {
    unsafe {
        let mut c = win::ProcessMemoryCounters::default();
        c.cb = std::mem::size_of::<win::ProcessMemoryCounters>() as u32;
        if win::K32GetProcessMemoryInfo(win::GetCurrentProcess(), &mut c, c.cb) != 0 {
            c.working_set_size as u64
        } else {
            0
        }
    }
}

// ── Total physical memory ──────────────────────────────────────────────────

#[cfg(target_os = "macos")]
fn total_physical_memory() -> u64 {
    unsafe {
        let mut mem: u64 = 0;
        let mut size = std::mem::size_of::<u64>();
        let name = c"hw.memsize";
        if libc::sysctlbyname(
            name.as_ptr(),
            &mut mem as *mut u64 as *mut libc::c_void,
            &mut size,
            std::ptr::null_mut(),
            0,
        ) == 0
        {
            return mem;
        }
        0
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn total_physical_memory() -> u64 {
    unsafe {
        let pages = libc::sysconf(libc::_SC_PHYS_PAGES);
        let page = libc::sysconf(libc::_SC_PAGESIZE);
        if pages > 0 && page > 0 {
            (pages as u64) * (page as u64)
        } else {
            0
        }
    }
}

#[cfg(windows)]
fn total_physical_memory() -> u64 {
    unsafe {
        let mut m = win::MemoryStatusEx::default();
        m.length = std::mem::size_of::<win::MemoryStatusEx>() as u32;
        if win::GlobalMemoryStatusEx(&mut m) != 0 {
            m.total_phys
        } else {
            0
        }
    }
}

// ── GPU ────────────────────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
fn gpu_percent() -> Option<f32> {
    // AMD / some Intel expose a device-wide busy% here; absent on many systems.
    for card in 0..4 {
        let path = format!("/sys/class/drm/card{card}/device/gpu_busy_percent");
        if let Ok(s) = std::fs::read_to_string(&path) {
            if let Ok(v) = s.trim().parse::<f32>() {
                return Some(v);
            }
        }
    }
    None
}

#[cfg(not(target_os = "linux"))]
fn gpu_percent() -> Option<f32> {
    // No portable per-process GPU metric (macOS / Windows need private APIs).
    None
}

// ── Windows kernel32 bindings (no external crate) ──────────────────────────

#[cfg(windows)]
mod win {
    use std::os::raw::c_void;

    #[repr(C)]
    #[derive(Default)]
    pub struct Filetime {
        pub low: u32,
        pub high: u32,
    }
    impl Filetime {
        pub fn as_ticks(&self) -> u64 {
            ((self.high as u64) << 32) | (self.low as u64)
        }
    }

    #[repr(C)]
    #[derive(Default)]
    pub struct ProcessMemoryCounters {
        pub cb: u32,
        pub page_fault_count: u32,
        pub peak_working_set_size: usize,
        pub working_set_size: usize,
        pub quota_peak_paged_pool_usage: usize,
        pub quota_paged_pool_usage: usize,
        pub quota_peak_non_paged_pool_usage: usize,
        pub quota_non_paged_pool_usage: usize,
        pub pagefile_usage: usize,
        pub peak_pagefile_usage: usize,
    }

    #[repr(C)]
    #[derive(Default)]
    pub struct MemoryStatusEx {
        pub length: u32,
        pub memory_load: u32,
        pub total_phys: u64,
        pub avail_phys: u64,
        pub total_page_file: u64,
        pub avail_page_file: u64,
        pub total_virtual: u64,
        pub avail_virtual: u64,
        pub avail_extended_virtual: u64,
    }

    unsafe extern "system" {
        pub fn GetCurrentProcess() -> *mut c_void;
        pub fn GetProcessTimes(
            process: *mut c_void,
            creation: *mut Filetime,
            exit: *mut Filetime,
            kernel: *mut Filetime,
            user: *mut Filetime,
        ) -> i32;
        pub fn K32GetProcessMemoryInfo(
            process: *mut c_void,
            counters: *mut ProcessMemoryCounters,
            cb: u32,
        ) -> i32;
        pub fn GlobalMemoryStatusEx(buffer: *mut MemoryStatusEx) -> i32;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sampler_returns_sane_values() {
        let mut s = Sampler::new();
        assert!(s.total_mem > 0, "total physical memory should be known");
        let _ = s.sample(); // first sample: cpu is 0 (needs delta)
        // busy-loop a hair so the second sample has a nonzero interval
        let start = std::time::Instant::now();
        let mut acc = 0u64;
        while start.elapsed().as_millis() < 30 {
            acc = acc.wrapping_add(1);
        }
        std::hint::black_box(acc);
        let m = s.sample();
        assert!(m.sampled);
        assert!(m.mem_mb > 0.0, "resident memory should be > 0, got {}", m.mem_mb);
        assert!(m.mem_pct >= 0.0 && m.mem_pct <= 100.0, "mem% out of range: {}", m.mem_pct);
        assert!(m.cpu_pct >= 0.0, "cpu% negative: {}", m.cpu_pct);
    }
}
