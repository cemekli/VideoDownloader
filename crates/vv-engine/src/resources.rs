//! What this PC has for local AI work, and whether a model fits it
//! (STATE.md F6.6; SPEC AI-A05, §17.1 `estimate_resources`; ADR-0033).
//!
//! - [`memory`] reads total and available RAM from the operating system
//!   (Windows `GlobalMemoryStatusEx`, Linux `/proc/meminfo`). Nothing else
//!   is read and nothing leaves the PC.
//! - [`fit`] compares a model's need with that: `TooBig` when the PC does
//!   not have the memory even with every other program closed, `Tight`
//!   when it does but not free right now. Neither starts without the
//!   user's explicit "use it anyway" (AI-A05: no large model is loaded
//!   silently on a low-memory PC).
//! - [`audio_disk_bytes`] is the staging space a transcription writes.
//! - [`CpuUse`] is the user's limit on the threads whisper.cpp may use;
//!   the process also runs at below-normal priority (vv-process).
//!
//! The need is an estimate built from whisper.cpp's own figure plus room
//! for DTW timestamps and the audio buffers, not a measurement (AI-A07).

use serde::{Deserialize, Serialize};

const MB: u64 = 1024 * 1024;

/// Memory the operating system and the app itself keep using while
/// whisper runs; a model that leaves less than this is `TooBig`.
pub const SYSTEM_RESERVE_MB: u64 = 1536;

/// Physical memory, in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Memory {
    pub total_bytes: u64,
    pub available_bytes: u64,
}

/// The PC's memory, or `None` when it cannot be read.
///
/// Debug builds honour `VV_FAKE_MEMORY_MB=<total>,<available>` so the
/// low-memory journey can be run on any PC (tests, e2e).
pub fn memory() -> Option<Memory> {
    if cfg!(debug_assertions) {
        if let Some(m) = std::env::var("VV_FAKE_MEMORY_MB")
            .ok()
            .and_then(|v| parse_fake(&v))
        {
            return Some(m);
        }
    }
    os_memory()
}

fn parse_fake(v: &str) -> Option<Memory> {
    let (t, a) = v.split_once(',')?;
    let total = t.trim().parse::<u64>().ok()? * MB;
    let available = a.trim().parse::<u64>().ok()? * MB;
    (total > 0 && available <= total).then_some(Memory {
        total_bytes: total,
        available_bytes: available,
    })
}

#[cfg(windows)]
fn os_memory() -> Option<Memory> {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    // SAFETY: MEMORYSTATUSEX is plain data; dwLength is set as the API
    // requires before the call, and the struct outlives it.
    let mut s: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
    s.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
    // SAFETY: `s` is a valid, initialised MEMORYSTATUSEX.
    let ok = unsafe { GlobalMemoryStatusEx(&mut s) };
    (ok != 0 && s.ullTotalPhys > 0).then_some(Memory {
        total_bytes: s.ullTotalPhys,
        available_bytes: s.ullAvailPhys.min(s.ullTotalPhys),
    })
}

#[cfg(not(windows))]
fn os_memory() -> Option<Memory> {
    parse_meminfo(&std::fs::read_to_string("/proc/meminfo").ok()?)
}

/// `/proc/meminfo`'s `MemTotal` and `MemAvailable` (kB).
pub fn parse_meminfo(text: &str) -> Option<Memory> {
    let field = |name: &str| {
        text.lines().find_map(|l| {
            let rest = l.strip_prefix(name)?.strip_prefix(':')?;
            let kb = rest.trim().strip_suffix("kB").unwrap_or(rest).trim();
            kb.parse::<u64>().ok().map(|k| k * 1024)
        })
    };
    let total = field("MemTotal")?;
    let available = field("MemAvailable")?;
    (total > 0).then_some(Memory {
        total_bytes: total,
        available_bytes: available.min(total),
    })
}

/// What a run of a model needs: whisper.cpp's listed figure, a quarter
/// more for DTW alignment without flash attention, and 128 MB for the
/// audio buffers.
pub fn need_mb(upstream_memory_mb: i64) -> u64 {
    let up = upstream_memory_mb.max(0) as u64;
    up + up / 4 + 128
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fit {
    /// Enough memory is free now.
    Ok,
    /// The PC has it, but not free right now (other programs use it).
    Tight,
    /// More than this PC can give, even with nothing else open.
    TooBig,
    /// The memory could not be read; nothing is claimed.
    Unknown,
}

impl Fit {
    /// Starting a run needs the user's explicit "use it anyway".
    pub fn needs_consent(self) -> bool {
        matches!(self, Fit::Tight | Fit::TooBig)
    }
}

pub fn fit(need_mb: u64, mem: Option<Memory>) -> Fit {
    let Some(m) = mem else {
        return Fit::Unknown;
    };
    let need = need_mb * MB;
    if m.total_bytes < need + SYSTEM_RESERVE_MB * MB {
        Fit::TooBig
    } else if m.available_bytes < need {
        Fit::Tight
    } else {
        Fit::Ok
    }
}

/// Staging space a transcription of `duration_ms` writes: the 16 kHz mono
/// 16-bit WAV (32 000 bytes a second), one part of at most `chunk_ms`
/// beside it, and 64 MB for whisper's JSON and the checkpoint.
pub fn audio_disk_bytes(duration_ms: i64, chunk_ms: i64) -> u64 {
    let pcm = |ms: i64| (ms.max(0) as u64) * 32;
    pcm(duration_ms) + pcm(duration_ms.min(chunk_ms)) + 64 * MB
}

pub use vv_persistence::settings::CpuUse;

/// Threads whisper.cpp gets under `u` on `cores` logical processors:
/// `Full` is all cores but one, at most 8 (F6.2's measured speeds; it
/// gains little beyond that on a CPU), `Balanced` half of `Full` (rounded
/// up; half the *cores* was also 8 on a 24-thread PC, the same as `Full`:
/// Cuma, 13 Nov 2026), `Light` two.
pub fn threads(u: CpuUse, cores: usize) -> usize {
    let full = cores.saturating_sub(1).clamp(1, 8);
    match u {
        CpuUse::Light => full.min(2),
        CpuUse::Balanced => full.div_ceil(2),
        CpuUse::Full => full,
    }
}

/// Logical processors of this PC.
pub fn cores() -> usize {
    std::thread::available_parallelism().map_or(1, |n| n.get())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem(total_mb: u64, avail_mb: u64) -> Option<Memory> {
        Some(Memory {
            total_bytes: total_mb * MB,
            available_bytes: avail_mb * MB,
        })
    }

    #[test]
    fn need_has_room_for_dtw_and_buffers() {
        assert_eq!(need_mb(2100), 2100 + 525 + 128);
        assert_eq!(need_mb(273), 273 + 68 + 128);
        assert_eq!(need_mb(-5), 128);
    }

    #[test]
    fn low_memory_pc_does_not_get_the_big_model_silently() {
        // A 4 GB PC: Medium cannot fit next to the system; Small can.
        let pc = mem(4096, 2500);
        assert_eq!(fit(need_mb(2100), pc), Fit::TooBig);
        assert!(fit(need_mb(2100), pc).needs_consent());
        assert_eq!(fit(need_mb(852), pc), Fit::Ok);
        // 16 GB, but a browser holds most of it right now.
        assert_eq!(fit(need_mb(2100), mem(16384, 1800)), Fit::Tight);
        assert_eq!(fit(need_mb(2100), mem(16384, 9000)), Fit::Ok);
        assert_eq!(fit(need_mb(2100), None), Fit::Unknown);
        assert!(!Fit::Unknown.needs_consent());
    }

    #[test]
    fn meminfo_is_read() {
        let t = "MemTotal:       16303428 kB\nMemFree:  1 kB\nMemAvailable:    9876543 kB\n";
        let m = parse_meminfo(t).unwrap();
        assert_eq!(m.total_bytes, 16_303_428 * 1024);
        assert_eq!(m.available_bytes, 9_876_543 * 1024);
        assert_eq!(parse_meminfo("MemTotal: 5 kB\n"), None);
        assert_eq!(parse_meminfo("garbage"), None);
    }

    #[test]
    fn fake_memory_is_checked() {
        assert_eq!(parse_fake("4096,1024"), mem(4096, 1024));
        assert_eq!(parse_fake("1024,4096"), None);
        assert_eq!(parse_fake("x,1"), None);
        assert_eq!(parse_fake("0,0"), None);
    }

    #[test]
    fn this_pc_reports_memory() {
        // Every CI runner (Windows, Linux) can tell its memory.
        let m = os_memory().expect("memory readable");
        assert!(m.total_bytes >= 512 * MB);
        assert!(m.available_bytes <= m.total_bytes);
    }

    #[test]
    fn disk_need_for_an_hour() {
        let ten_min = 600_000;
        let b = audio_disk_bytes(3_600_000, ten_min);
        assert_eq!(b, 3_600_000 * 32 + 600_000 * 32 + 64 * MB);
        assert_eq!(audio_disk_bytes(1_000, ten_min), 2_000 * 32 + 64 * MB);
    }

    #[test]
    fn cpu_use_threads() {
        assert_eq!(threads(CpuUse::Full, 16), 8);
        assert_eq!(threads(CpuUse::Balanced, 16), 4);
        assert_eq!(threads(CpuUse::Balanced, 24), 4);
        assert_eq!(threads(CpuUse::Full, 24), 8);
        assert_eq!(threads(CpuUse::Balanced, 8), 4);
        assert_eq!(threads(CpuUse::Light, 8), 2);
        assert_eq!(threads(CpuUse::Full, 4), 3);
        assert_eq!(threads(CpuUse::Balanced, 2), 1);
        for c in 1..=4 {
            for u in [CpuUse::Light, CpuUse::Balanced, CpuUse::Full] {
                assert!(threads(u, c) >= 1);
            }
        }
    }
}
