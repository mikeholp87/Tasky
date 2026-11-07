//! OS-independent logic for on-demand process inspection. Platform code
//! gathers raw facts (regions, thread times); this module classifies and
//! summarizes them so the same rules, and the same tests, apply everywhere.

use std::collections::HashMap;
use std::time::Instant;

use crate::errors::{AppError, AppResult};
use crate::models::{
    MemoryKindTotal, MemoryMap, MemoryRegion, MemoryRegionKind, ModuleInfo, ProcessState,
    ThreadInfo, ThreadList,
};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

/// Numeric-PID reads must not return a replacement process's data.
pub fn for_process<T>(
    pid: u32,
    start_time: u64,
    read: impl FnOnce() -> AppResult<T>,
) -> AppResult<T> {
    if start_time == 0 {
        return Err(AppError::ActionUnavailable(
            "The process start time is unavailable, so its identity cannot be verified.".into(),
        ));
    }
    let mut system = System::new().map_err(|e| AppError::SystemQueryFailed(e.to_string()))?;
    let mut check = || {
        system.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[Pid::from_u32(pid)]),
            true,
            ProcessRefreshKind::nothing().without_tasks(),
        );
        system
            .process(Pid::from_u32(pid))
            .filter(|p| p.start_time() == start_time)
            .map(|_| ())
            .ok_or(AppError::ProcessNotFound(pid))
    };
    check()?;
    let result = read();
    check()?;
    result
}

/// Row caps keep a pathological process (hundreds of thousands of mappings)
/// from producing an unusable IPC payload.
pub const MAX_REGIONS: usize = 20_000;
pub const MAX_SOCKETS: usize = 20_000;
pub const MAX_OPEN_FILES: usize = 5_000;
pub const MAX_FILE_USES: usize = 2_000;

/// A region as gathered by a platform, before it is rendered for the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Region {
    pub start: u64,
    pub end: u64,
    pub perms: String,
    pub offset: u64,
    pub path: Option<String>,
    pub kind: MemoryRegionKind,
    pub shared: bool,
}

impl Region {
    pub fn size(&self) -> u64 {
        self.end.saturating_sub(self.start)
    }

    pub fn executable(&self) -> bool {
        self.perms.as_bytes().get(2) == Some(&b'x')
    }
}

/// Classifies a Linux mapping. Only labels the kernel provides are trusted:
/// thread stacks and malloc arenas are indistinguishable from other
/// anonymous memory and stay `Anonymous`.
pub fn classify_linux_region(path: Option<&str>, exe: Option<&str>) -> MemoryRegionKind {
    let Some(path) = path else {
        return MemoryRegionKind::Anonymous;
    };
    let clean = path.strip_suffix(" (deleted)").unwrap_or(path);
    match clean {
        "[heap]" => return MemoryRegionKind::Heap,
        "[vdso]" | "[vvar]" | "[vvar_vclock]" | "[vsyscall]" => return MemoryRegionKind::Special,
        _ => {}
    }
    if clean.starts_with("[stack") {
        return MemoryRegionKind::Stack;
    }
    if clean.starts_with('[') {
        // [anon:...] names set by allocators, [uprobes], ...
        return MemoryRegionKind::Anonymous;
    }
    if exe.is_some_and(|e| e.strip_suffix(" (deleted)").unwrap_or(e) == clean) {
        return MemoryRegionKind::Image;
    }
    let file = clean.rsplit('/').next().unwrap_or(clean);
    if file.ends_with(".so") || file.contains(".so.") {
        MemoryRegionKind::SharedLibrary
    } else {
        MemoryRegionKind::MappedFile
    }
}

/// `rwx`-style permissions from a Windows `PAGE_*` protection value. Copy-on-
/// write counts as writable; guard and cache modifier bits are ignored.
pub fn windows_protection(protect: u32) -> String {
    let (r, w, x) = match protect & 0xFF {
        0x02 => (true, false, false),
        0x04 | 0x08 => (true, true, false),
        0x10 => (false, false, true),
        0x20 => (true, false, true),
        0x40 | 0x80 => (true, true, true),
        _ => (false, false, false), // PAGE_NOACCESS or unknown
    };
    let c = |on, ch| if on { ch } else { '-' };
    [c(r, 'r'), c(w, 'w'), c(x, 'x')].iter().collect()
}

/// Removes the `\\?\` (or `\\?\UNC\`) prefix `canonicalize` adds on Windows,
/// which most Win32 file APIs and users do not expect.
pub fn strip_verbatim_prefix(path: &str) -> String {
    if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else {
        path.strip_prefix(r"\\?\").unwrap_or(path).to_owned()
    }
}

/// Builds the UI model: raw regions plus per-kind totals.
pub fn build_memory_map(mut regions: Vec<Region>) -> MemoryMap {
    let truncated = regions.len() > MAX_REGIONS;
    regions.truncate(MAX_REGIONS);

    let mut totals: HashMap<MemoryRegionKind, (u64, u32)> = HashMap::new();
    let (mut total_bytes, mut executable_bytes) = (0u64, 0u64);
    for r in &regions {
        let entry = totals.entry(r.kind).or_default();
        entry.0 = entry.0.saturating_add(r.size());
        entry.1 += 1;
        total_bytes = total_bytes.saturating_add(r.size());
        if r.executable() {
            executable_bytes = executable_bytes.saturating_add(r.size());
        }
    }
    let mut summary: Vec<MemoryKindTotal> = totals
        .into_iter()
        .map(|(kind, (bytes, regions))| MemoryKindTotal {
            kind,
            bytes,
            regions,
        })
        .collect();
    summary.sort_by(|a, b| b.bytes.cmp(&a.bytes).then(a.regions.cmp(&b.regions)));

    MemoryMap {
        regions: regions
            .into_iter()
            .map(|r| MemoryRegion {
                start: format!("{:x}", r.start),
                end: format!("{:x}", r.end),
                size: r.size(),
                permissions: r.perms,
                offset: r.offset,
                path: r.path,
                kind: r.kind,
                shared: r.shared,
            })
            .collect(),
        summary,
        total_bytes,
        executable_bytes,
        truncated,
    }
}

/// Modules of a Linux process: the executable and shared libraries, one row
/// per file however many mappings it has. Other file mappings (data files,
/// shared memory) are not modules and stay visible in the memory map.
pub fn modules_from_regions(regions: &[Region]) -> Vec<ModuleInfo> {
    let mut by_path: HashMap<&str, (u64, u64, bool)> = HashMap::new();
    for r in regions {
        if !matches!(
            r.kind,
            MemoryRegionKind::Image | MemoryRegionKind::SharedLibrary
        ) {
            continue;
        }
        let Some(path) = r.path.as_deref() else {
            continue;
        };
        let e = by_path.entry(path).or_insert((u64::MAX, 0, false));
        e.0 = e.0.min(r.start);
        e.1 = e.1.saturating_add(r.size());
        e.2 |= r.executable();
    }
    let mut out: Vec<(u64, ModuleInfo)> = by_path
        .into_iter()
        .map(|(path, (base, size, executable))| {
            let clean = path.strip_suffix(" (deleted)").unwrap_or(path);
            (
                base,
                ModuleInfo {
                    name: clean.rsplit('/').next().unwrap_or(clean).to_owned(),
                    path: path.to_owned(),
                    base: format!("{base:x}"),
                    size,
                    executable,
                },
            )
        })
        .collect();
    out.sort_by_key(|(base, _)| *base);
    out.into_iter().map(|(_, m)| m).collect()
}

/// A thread as read from the OS, with cumulative CPU time only.
#[derive(Debug, Clone)]
pub struct RawThread {
    pub tid: u32,
    pub name: Option<String>,
    pub state: Option<ProcessState>,
    pub cpu_time_ms: Option<u64>,
    pub priority: Option<i32>,
    pub start_time: Option<u64>,
    /// Native creation counter preserves identity within the same display second.
    pub start_id: Option<u64>,
    pub processor: Option<u32>,
    pub wait_channel: Option<String>,
}

/// Turns cumulative thread CPU times into percentages by comparing with the
/// previous reading of the *same process instance*. Holds one process only:
/// per-thread sampling is on demand, for the process being inspected.
#[derive(Default)]
pub struct ThreadSampler {
    last: Option<Last>,
}

struct Last {
    pid: u32,
    process_start: u64,
    at: Instant,
    cpu_ms: HashMap<(u32, u64), u64>,
}

impl ThreadSampler {
    /// `cores` is the logical CPU count; percentages are of the whole machine,
    /// matching the Processes page.
    pub fn sample(
        &mut self,
        pid: u32,
        process_start: u64,
        raw: Vec<RawThread>,
        now: Instant,
        sampled_at_ms: u64,
        cores: usize,
    ) -> ThreadList {
        let previous = self
            .last
            .take()
            .filter(|l| l.pid == pid && l.process_start == process_start);
        let elapsed_ms = previous
            .as_ref()
            .map(|l| now.saturating_duration_since(l.at).as_secs_f64() * 1_000.0);

        let mut cpu_ms = HashMap::with_capacity(raw.len());
        let mut threads: Vec<ThreadInfo> = raw
            .into_iter()
            .map(|t| {
                let reading = t
                    .start_id
                    .zip(t.cpu_time_ms)
                    .map(|(start, cpu)| ((t.tid, start), cpu));
                if let Some((identity, cpu)) = reading {
                    cpu_ms.insert(identity, cpu);
                }
                let cpu_percent = reading
                    .zip(previous.as_ref().zip(elapsed_ms))
                    // Too short an interval gives meaningless ratios.
                    .filter(|(_, (_, e))| *e >= 100.0)
                    .and_then(|((identity, cpu), (l, e))| {
                        let before = l.cpu_ms.get(&identity)?;
                        let used = cpu.checked_sub(*before)? as f64;
                        Some((used / e * 100.0 / cores.max(1) as f64).clamp(0.0, 100.0) as f32)
                    });
                ThreadInfo {
                    tid: t.tid,
                    name: t.name,
                    state: t.state,
                    cpu_percent,
                    cpu_time_ms: t.cpu_time_ms,
                    priority: t.priority,
                    start_time: t.start_time,
                    processor: t.processor,
                    wait_channel: t.wait_channel,
                }
            })
            .collect();
        threads.sort_by(|a, b| {
            b.cpu_percent
                .partial_cmp(&a.cpu_percent)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(b.cpu_time_ms.cmp(&a.cpu_time_ms))
                .then(a.tid.cmp(&b.tid))
        });
        // Keep this reading as the baseline only when it was usable; a reading
        // taken too soon would shrink the next window.
        if elapsed_ms.is_none_or(|e| e >= 100.0) {
            self.last = Some(Last {
                pid,
                process_start,
                at: now,
                cpu_ms,
            });
        } else {
            self.last = previous;
        }
        ThreadList {
            threads,
            sampled_at_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn unverified_process_identities_never_run_the_inspector() {
        for start_time in [0, u64::MAX] {
            let mut inspected = false;
            assert!(
                for_process(std::process::id(), start_time, || {
                    inspected = true;
                    Ok(())
                })
                .is_err()
            );
            assert!(!inspected);
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_process_that_exits_during_inspection_cannot_return_data() {
        let mut child = std::process::Command::new("sleep")
            .arg("60")
            .spawn()
            .unwrap();
        let pid = child.id();
        let mut system = System::new().unwrap();
        system.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[Pid::from_u32(pid)]),
            true,
            ProcessRefreshKind::nothing().without_tasks(),
        );
        let start_time = system.process(Pid::from_u32(pid)).unwrap().start_time();
        let result = for_process(pid, start_time, || {
            child.kill().unwrap();
            child.wait().unwrap();
            Ok("must not escape")
        });
        let _ = child.kill();
        let _ = child.wait();
        assert!(matches!(result, Err(AppError::ProcessNotFound(found)) if found == pid));
    }

    fn region(start: u64, end: u64, perms: &str, path: Option<&str>, exe: Option<&str>) -> Region {
        Region {
            start,
            end,
            perms: perms.into(),
            offset: 0,
            path: path.map(str::to_owned),
            kind: classify_linux_region(path, exe),
            shared: perms.ends_with('s'),
        }
    }

    #[test]
    fn classifies_linux_mappings() {
        use MemoryRegionKind as K;
        let exe = Some("/usr/bin/app");
        assert_eq!(classify_linux_region(None, exe), K::Anonymous);
        assert_eq!(classify_linux_region(Some("[heap]"), exe), K::Heap);
        assert_eq!(classify_linux_region(Some("[stack]"), exe), K::Stack);
        assert_eq!(classify_linux_region(Some("[stack:1234]"), exe), K::Stack);
        assert_eq!(classify_linux_region(Some("[vdso]"), exe), K::Special);
        assert_eq!(
            classify_linux_region(Some("[anon:scudo]"), exe),
            K::Anonymous
        );
        assert_eq!(classify_linux_region(Some("/usr/bin/app"), exe), K::Image);
        assert_eq!(
            classify_linux_region(Some("/usr/bin/app (deleted)"), exe),
            K::Image
        );
        assert_eq!(
            classify_linux_region(Some("/usr/lib/libc.so.6"), exe),
            K::SharedLibrary
        );
        assert_eq!(
            classify_linux_region(Some("/opt/x/plugin.so"), exe),
            K::SharedLibrary
        );
        assert_eq!(
            classify_linux_region(Some("/home/a/data.db"), exe),
            K::MappedFile
        );
        assert_eq!(
            classify_linux_region(Some("/usr/bin/other"), None),
            K::MappedFile
        );
    }

    #[test]
    fn maps_windows_protection_and_paths() {
        assert_eq!(windows_protection(0x02), "r--");
        assert_eq!(windows_protection(0x04), "rw-");
        assert_eq!(windows_protection(0x20), "r-x");
        assert_eq!(windows_protection(0x40 | 0x100), "rwx", "guard bit ignored");
        assert_eq!(windows_protection(0x01), "---");
        assert_eq!(
            strip_verbatim_prefix(r"\\?\C:\Temp\a.txt"),
            r"C:\Temp\a.txt"
        );
        assert_eq!(
            strip_verbatim_prefix(r"\\?\UNC\srv\share\a"),
            r"\\srv\share\a"
        );
        assert_eq!(strip_verbatim_prefix(r"C:\plain"), r"C:\plain");
    }

    #[test]
    fn summarizes_and_counts_executable_bytes() {
        let regions = vec![
            region(
                0x1000,
                0x3000,
                "r-xp",
                Some("/usr/bin/app"),
                Some("/usr/bin/app"),
            ),
            region(0x10000, 0x90000, "rw-p", Some("[heap]"), None),
            region(0x90000, 0x91000, "rw-p", None, None),
            region(0x91000, 0x92000, "rw-s", Some("/dev/shm/x"), None),
        ];
        let map = build_memory_map(regions);
        assert_eq!(map.total_bytes, 0x2000 + 0x80000 + 0x1000 + 0x1000);
        assert_eq!(map.executable_bytes, 0x2000);
        assert_eq!(map.summary[0].kind, MemoryRegionKind::Heap);
        assert_eq!(map.regions[0].start, "1000");
        assert!(map.regions[3].shared);
        assert!(!map.truncated);
        assert!(build_memory_map(Vec::new()).summary.is_empty());
    }

    #[test]
    fn groups_mappings_into_modules() {
        let exe = Some("/usr/bin/app");
        let regions = vec![
            region(0x5000, 0x6000, "r--p", Some("/usr/lib/libz.so.1"), exe),
            region(0x6000, 0x8000, "r-xp", Some("/usr/lib/libz.so.1"), exe),
            region(0x1000, 0x2000, "r-xp", Some("/usr/bin/app"), exe),
            region(0x9000, 0xa000, "rw-s", Some("/home/a/data.db"), exe),
            region(0xb000, 0xc000, "rw-p", None, exe),
        ];
        let modules = modules_from_regions(&regions);
        assert_eq!(
            modules.len(),
            2,
            "data files and anonymous memory are not modules"
        );
        assert_eq!(modules[0].name, "app");
        assert_eq!(modules[1].name, "libz.so.1");
        assert_eq!(modules[1].base, "5000");
        assert_eq!(modules[1].size, 0x3000);
        assert!(modules[1].executable);
    }

    fn raw(tid: u32, cpu_time_ms: u64) -> RawThread {
        RawThread {
            tid,
            name: None,
            state: Some(ProcessState::Running),
            cpu_time_ms: Some(cpu_time_ms),
            priority: None,
            start_time: Some(1),
            start_id: Some(1),
            processor: None,
            wait_channel: None,
        }
    }

    #[test]
    fn thread_cpu_needs_two_samples_and_scales_by_cores() {
        let mut s = ThreadSampler::default();
        let t0 = Instant::now();
        let first = s.sample(7, 100, vec![raw(1, 1_000), raw(2, 0)], t0, 0, 4);
        assert!(first.threads.iter().all(|t| t.cpu_percent.is_none()));

        // 1 s later thread 1 used 800 ms (one core 80% busy on 4 cores = 20%).
        let t1 = t0 + Duration::from_secs(1);
        let second = s.sample(
            7,
            100,
            vec![raw(1, 1_800), raw(2, 0), raw(3, 100)],
            t1,
            1_000,
            4,
        );
        let by_tid = |tid| second.threads.iter().find(|t| t.tid == tid).unwrap();
        assert!((by_tid(1).cpu_percent.unwrap() - 20.0).abs() < 0.01);
        assert_eq!(by_tid(2).cpu_percent, Some(0.0));
        assert_eq!(
            by_tid(3).cpu_percent,
            None,
            "a new thread needs its own baseline"
        );
        assert_eq!(second.threads[0].tid, 1, "busiest thread first");
    }

    #[test]
    fn thread_sampler_resets_for_a_new_process_instance() {
        let mut s = ThreadSampler::default();
        let t0 = Instant::now();
        s.sample(7, 100, vec![raw(1, 5_000)], t0, 0, 1);
        let later = t0 + Duration::from_secs(1);
        // Same pid, different start time: PID reuse, so no stale baseline.
        let reused = s.sample(7, 999, vec![raw(1, 10)], later, 1_000, 1);
        assert_eq!(reused.threads[0].cpu_percent, None);
    }

    #[test]
    fn rapid_resamples_keep_the_older_baseline() {
        let mut s = ThreadSampler::default();
        let t0 = Instant::now();
        s.sample(1, 1, vec![raw(1, 0)], t0, 0, 1);
        let soon = s.sample(1, 1, vec![raw(1, 5)], t0 + Duration::from_millis(10), 10, 1);
        assert_eq!(soon.threads[0].cpu_percent, None);
        let later = s.sample(
            1,
            1,
            vec![raw(1, 500)],
            t0 + Duration::from_secs(1),
            1_000,
            1,
        );
        assert!((later.threads[0].cpu_percent.unwrap() - 50.0).abs() < 0.01);
    }

    #[test]
    fn unavailable_cpu_and_reused_threads_never_create_false_baselines() {
        let mut sampler = ThreadSampler::default();
        let now = Instant::now();
        let mut unavailable = raw(1, 0);
        unavailable.cpu_time_ms = None;
        sampler.sample(7, 100, vec![unavailable.clone()], now, 0, 1);
        let absent = sampler.sample(
            7,
            100,
            vec![unavailable],
            now + Duration::from_secs(1),
            1_000,
            1,
        );
        assert_eq!(absent.threads[0].cpu_percent, None);
        assert_eq!(absent.threads[0].cpu_time_ms, None);
        let first = sampler.sample(
            7,
            100,
            vec![raw(1, 50_000)],
            now + Duration::from_secs(2),
            2_000,
            1,
        );
        assert_eq!(first.threads[0].cpu_percent, None);
        let known = sampler.sample(
            7,
            100,
            vec![raw(1, 50_100)],
            now + Duration::from_secs(3),
            3_000,
            1,
        );
        assert_eq!(known.threads[0].cpu_percent, Some(10.0));
        let mut replacement = raw(1, 1_000);
        replacement.start_id = Some(2);
        let reused = sampler.sample(
            7,
            100,
            vec![replacement],
            now + Duration::from_secs(4),
            4_000,
            1,
        );
        assert_eq!(reused.threads[0].cpu_percent, None);
    }
}
