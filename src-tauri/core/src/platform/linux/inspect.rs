//! On-demand inspection from `/proc`: sockets, open files, memory maps,
//! modules and threads. Nothing here runs on the polling path.
//!
//! Other users' processes are unreadable without privileges. Those are
//! counted and reported, never treated as "not using anything".

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, ErrorKind};
use std::time::{Duration, Instant};

use crate::errors::{AppError, AppResult};
use crate::inspect::{
    MAX_FILE_USES, MAX_OPEN_FILES, MAX_SOCKETS, RawThread, Region, build_memory_map,
    classify_linux_region, modules_from_regions,
};
use crate::models::{
    FileUsage, FileUse, FileUserList, MemoryMap, ModuleInfo, OpenFile, OpenFileKind, OpenFileList,
    ProcessState, Protocol, SocketEntry, SocketList,
};
use crate::platform::procfs;

use super::connections::to_connection;

/// Longest a path search may scan before returning what it has.
const SEARCH_BUDGET: Duration = Duration::from_secs(4);

fn map_io(e: &io::Error, pid: u32) -> AppError {
    match e.kind() {
        ErrorKind::NotFound => AppError::ProcessNotFound(pid),
        ErrorKind::PermissionDenied => AppError::PermissionDenied(format!(
            "Access to process {pid} is denied. Details of other users' processes need root."
        )),
        _ => AppError::SystemQueryFailed(format!("process {pid}: {e}")),
    }
}

fn pids() -> Vec<u32> {
    fs::read_dir("/proc")
        .map(|d| {
            d.flatten()
                .filter_map(|e| e.file_name().to_str()?.parse().ok())
                .collect()
        })
        .unwrap_or_default()
}

fn comm(pid: u32) -> Option<String> {
    let text = fs::read_to_string(format!("/proc/{pid}/comm")).ok()?;
    Some(text.trim_end().to_owned())
}

fn link_string(path: &str) -> Option<String> {
    Some(fs::read_link(path).ok()?.to_string_lossy().into_owned())
}

// ----------------------------------------------------------------- sockets

/// Every TCP/UDP socket in this network namespace with its owning process.
///
/// Ownership comes from scanning each process's fd directory, so the cost
/// grows with the number of descriptors on the machine; callers poll slowly
/// and only while the Ports page is open.
pub fn sockets() -> AppResult<SocketList> {
    // ponytail: a socket inherited by several processes shows one owner (the
    // last scanned). Listing every holder would need a multimap and UI rows
    // per holder; add if users report confusion with forking servers.
    let mut owner: HashMap<u64, u32> = HashMap::new();
    let mut inaccessible = 0u32;
    for pid in pids() {
        match fs::read_dir(format!("/proc/{pid}/fd")) {
            Ok(dir) => {
                for entry in dir.flatten() {
                    if let Some(inode) = fs::read_link(entry.path())
                        .ok()
                        .and_then(|t| procfs::socket_inode(&t.to_string_lossy()))
                    {
                        owner.insert(inode, pid);
                    }
                }
            }
            Err(e) if e.kind() == ErrorKind::PermissionDenied => inaccessible += 1,
            Err(_) => {} // exited while scanning
        }
    }

    let mut names: HashMap<u32, Option<String>> = HashMap::new();
    let mut sockets = Vec::new();
    let mut truncated = false;
    'files: for (file, protocol) in [
        ("tcp", Protocol::Tcp),
        ("tcp6", Protocol::Tcp6),
        ("udp", Protocol::Udp),
        ("udp6", Protocol::Udp6),
    ] {
        let Ok(text) = fs::read_to_string(format!("/proc/net/{file}")) else {
            continue;
        };
        for entry in text.lines().filter_map(procfs::parse_net_line) {
            if sockets.len() >= MAX_SOCKETS {
                truncated = true;
                break 'files;
            }
            let pid = owner
                .get(&entry.inode)
                .copied()
                .filter(|_| entry.inode != 0);
            let process_name = pid.and_then(|p| names.entry(p).or_insert_with(|| comm(p)).clone());
            sockets.push(SocketEntry {
                connection: to_connection(&entry, protocol),
                pid,
                process_name,
            });
        }
    }
    Ok(SocketList {
        sockets,
        inaccessible_processes: inaccessible,
        truncated,
    })
}

// ------------------------------------------------------------- open files

pub fn open_files(pid: u32) -> AppResult<OpenFileList> {
    let dir = fs::read_dir(format!("/proc/{pid}/fd")).map_err(|e| map_io(&e, pid))?;
    let mut files = Vec::new();
    let mut truncated = false;
    for entry in dir.flatten() {
        if files.len() >= MAX_OPEN_FILES {
            truncated = true;
            break;
        }
        let descriptor = entry.file_name().to_string_lossy().into_owned();
        // Closed between listing and reading.
        let Some(target) = link_string(&entry.path().to_string_lossy()) else {
            continue;
        };
        let mut kind = procfs::classify_fd_target(&target);
        if kind == OpenFileKind::File && fs::metadata(entry.path()).is_ok_and(|m| m.is_dir()) {
            kind = OpenFileKind::Directory;
        }
        let access = (kind != OpenFileKind::Socket)
            .then(|| fs::read_to_string(format!("/proc/{pid}/fdinfo/{descriptor}")).ok())
            .flatten()
            .and_then(|t| procfs::parse_fdinfo_access(&t));
        files.push(OpenFile {
            descriptor,
            kind,
            target,
            access,
        });
    }
    files.sort_by_key(|f| f.descriptor.parse::<u32>().unwrap_or(u32::MAX));
    Ok(OpenFileList { files, truncated })
}

/// Which processes use `path`: open descriptors, working directory,
/// executable and memory-mapped files. A directory matches everything
/// beneath it too.
pub fn find_file_users(path: &str) -> AppResult<FileUserList> {
    // ponytail: matched by canonical path text, not by device+inode, so a
    // second hard link or bind-mount alias of the same file is not found.
    let resolved = fs::canonicalize(path).map_err(|e| match e.kind() {
        ErrorKind::NotFound => AppError::InvalidArgument("That path does not exist.".into()),
        ErrorKind::PermissionDenied => {
            AppError::PermissionDenied("You do not have permission to read that path.".into())
        }
        _ => AppError::SystemQueryFailed(format!("cannot resolve path: {e}")),
    })?;
    let is_directory = fs::metadata(&resolved).is_ok_and(|m| m.is_dir());
    let query = resolved.to_string_lossy().into_owned();

    let started = Instant::now();
    let mut uses: Vec<FileUse> = Vec::new();
    let mut inaccessible = 0u32;
    let mut truncated = false;

    'procs: for pid in pids() {
        if started.elapsed() > SEARCH_BUDGET || uses.len() >= MAX_FILE_USES {
            truncated = true;
            break;
        }
        // First reason wins per (process, path): an mmap'd file that is also
        // open is reported once, as open.
        let mut seen: HashSet<String> = HashSet::new();
        let mut found: Vec<(FileUsage, String)> = Vec::new();
        let mut add = |usage: FileUsage, target: &str| {
            let clean = target.strip_suffix(" (deleted)").unwrap_or(target);
            if procfs::path_matches(clean, &query, is_directory) && seen.insert(clean.to_owned()) {
                found.push((usage, clean.to_owned()));
            }
        };

        match fs::read_dir(format!("/proc/{pid}/fd")) {
            Ok(dir) => {
                for entry in dir.flatten() {
                    if let Some(t) = link_string(&entry.path().to_string_lossy()) {
                        add(FileUsage::Open, &t);
                    }
                }
            }
            Err(e) if e.kind() == ErrorKind::PermissionDenied => {
                inaccessible += 1;
                continue 'procs;
            }
            Err(_) => continue 'procs,
        }
        if let Some(t) = link_string(&format!("/proc/{pid}/cwd")) {
            add(FileUsage::WorkingDirectory, &t);
        }
        if let Some(t) = link_string(&format!("/proc/{pid}/exe")) {
            add(FileUsage::Executable, &t);
        }
        if let Ok(maps) = fs::read_to_string(format!("/proc/{pid}/maps")) {
            for entry in maps.lines().filter_map(procfs::parse_maps_line) {
                if let Some(p) = entry.path.as_deref().filter(|p| p.starts_with('/')) {
                    add(FileUsage::Mapped, p);
                }
            }
        }

        if !found.is_empty() {
            let process_name = comm(pid).unwrap_or_default();
            uses.extend(found.into_iter().map(|(usage, path)| FileUse {
                pid,
                process_name: process_name.clone(),
                usage,
                path,
            }));
        }
    }
    uses.truncate(MAX_FILE_USES);
    uses.sort_by(|a, b| a.pid.cmp(&b.pid).then_with(|| a.path.cmp(&b.path)));

    Ok(FileUserList {
        resolved_path: query,
        is_directory,
        uses,
        inaccessible_processes: inaccessible,
        truncated,
        note: None,
    })
}

// ------------------------------------------------------- maps and modules

fn regions(pid: u32) -> AppResult<Vec<Region>> {
    let text = fs::read_to_string(format!("/proc/{pid}/maps")).map_err(|e| map_io(&e, pid))?;
    let exe = link_string(&format!("/proc/{pid}/exe"));
    Ok(text
        .lines()
        .filter_map(procfs::parse_maps_line)
        .map(|e| Region {
            kind: classify_linux_region(e.path.as_deref(), exe.as_deref()),
            shared: e.perms.ends_with('s'),
            start: e.start,
            end: e.end,
            perms: e.perms,
            offset: e.offset,
            path: e.path,
        })
        .collect())
}

pub fn memory_map(pid: u32) -> AppResult<MemoryMap> {
    Ok(build_memory_map(regions(pid)?))
}

pub fn modules(pid: u32) -> AppResult<Vec<ModuleInfo>> {
    Ok(modules_from_regions(&regions(pid)?))
}

// ---------------------------------------------------------------- threads

fn clock_ticks_per_sec() -> u64 {
    // SAFETY: sysconf only reads a system constant.
    let n = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    u64::try_from(n).ok().filter(|&n| n > 0).unwrap_or(100)
}

fn boot_time_secs() -> Option<u64> {
    fs::read_to_string("/proc/stat")
        .ok()?
        .lines()
        .find_map(|l| l.strip_prefix("btime "))?
        .trim()
        .parse()
        .ok()
}

fn thread_state(c: char) -> ProcessState {
    match c {
        'R' => ProcessState::Running,
        'S' => ProcessState::Sleeping,
        'D' => ProcessState::DiskSleep,
        'T' => ProcessState::Stopped,
        't' => ProcessState::Tracing,
        'Z' => ProcessState::Zombie,
        'X' | 'x' => ProcessState::Dead,
        'I' => ProcessState::Idle,
        _ => ProcessState::Unknown,
    }
}

pub fn threads(pid: u32) -> AppResult<Vec<RawThread>> {
    let dir = fs::read_dir(format!("/proc/{pid}/task")).map_err(|e| map_io(&e, pid))?;
    let ticks = clock_ticks_per_sec();
    let boot = boot_time_secs();
    let mut out = Vec::new();
    for entry in dir.flatten() {
        let Some(tid) = entry
            .file_name()
            .to_str()
            .and_then(|s| s.parse::<u32>().ok())
        else {
            continue;
        };
        // Threads exit all the time; one vanishing mid-read is not an error.
        let Some(stat) = fs::read_to_string(format!("/proc/{pid}/task/{tid}/stat"))
            .ok()
            .and_then(|t| procfs::parse_task_stat(&t))
        else {
            continue;
        };
        let wait_channel = fs::read_to_string(format!("/proc/{pid}/task/{tid}/wchan"))
            .ok()
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty() && s != "0");
        out.push(RawThread {
            tid,
            name: Some(stat.comm),
            state: Some(thread_state(stat.state)),
            cpu_time_ms: stat.cpu_ticks.saturating_mul(1_000) / ticks,
            priority: Some(stat.nice),
            start_time: boot.map(|b| b + stat.start_ticks / ticks),
            processor: Some(stat.processor),
            wait_channel,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::net::TcpListener;

    #[test]
    fn finds_own_listening_socket() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let list = sockets().unwrap();
        let mine = list
            .sockets
            .iter()
            .find(|s| s.connection.local_port == port && s.pid == Some(std::process::id()))
            .expect("own listener is listed with its owner");
        assert_eq!(mine.connection.state, Some(crate::models::TcpState::Listen));
        assert!(mine.process_name.is_some());
    }

    #[test]
    fn lists_own_open_file_with_access() {
        let path = std::env::temp_dir().join(format!("tm-open-{}", std::process::id()));
        let mut file = fs::File::create(&path).unwrap();
        file.write_all(b"x").unwrap();
        let list = open_files(std::process::id()).unwrap();
        let target = fs::canonicalize(&path)
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let found = list.files.iter().find(|f| f.target == target).unwrap();
        assert_eq!(found.kind, OpenFileKind::File);
        assert_eq!(found.access, Some(crate::models::FileAccess::Write));
        drop(file);
        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn finds_this_process_as_user_of_a_file_and_of_its_directory() {
        let dir = std::env::temp_dir().join(format!("tm-find-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("held.txt");
        let file = fs::File::create(&path).unwrap();

        let by_file = find_file_users(path.to_str().unwrap()).unwrap();
        assert!(!by_file.is_directory);
        assert!(by_file.uses.iter().any(|u| {
            u.pid == std::process::id()
                && u.usage == FileUsage::Open
                && u.path.ends_with("held.txt")
        }));

        let by_dir = find_file_users(dir.to_str().unwrap()).unwrap();
        assert!(by_dir.is_directory);
        assert!(by_dir.uses.iter().any(|u| u.pid == std::process::id()));

        drop(file);
        fs::remove_dir_all(&dir).unwrap();
        assert!(find_file_users("/definitely/not/here").is_err());
    }

    #[test]
    fn own_memory_map_has_heap_stack_and_libraries() {
        let map = memory_map(std::process::id()).unwrap();
        assert!(map.total_bytes > 0 && map.executable_bytes > 0);
        assert!(
            map.summary
                .iter()
                .any(|s| s.kind == crate::models::MemoryRegionKind::Stack)
        );
        let modules = modules(std::process::id()).unwrap();
        assert!(modules.iter().any(|m| m.name.starts_with("libc")));
    }

    #[test]
    fn counts_threads_of_this_process() {
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let handle = std::thread::spawn(move || {
            let _ = rx.recv();
        });
        std::thread::sleep(Duration::from_millis(50));
        let before = threads(std::process::id()).unwrap();
        assert!(before.len() >= 2);
        assert!(
            before
                .iter()
                .all(|t| t.state.is_some() && t.processor.is_some())
        );
        tx.send(()).unwrap();
        handle.join().unwrap();
    }

    /// `cargo test -p tm-core --release measure_inspection_cost -- --ignored --nocapture`
    #[test]
    #[ignore = "measurement, not a check"]
    fn measure_inspection_cost() {
        let time = |label: &str, f: &dyn Fn() -> usize| {
            let t = Instant::now();
            let n = f();
            eprintln!("{label}: {n} rows in {:?}", t.elapsed());
        };
        let me = std::process::id();
        time("sockets (all processes)", &|| {
            sockets().unwrap().sockets.len()
        });
        time("find_file_users(/usr)", &|| {
            find_file_users("/usr").unwrap().uses.len()
        });
        time("find_file_users(/etc/hostname)", &|| {
            find_file_users("/etc/hostname").unwrap().uses.len()
        });
        time("open_files(self)", &|| open_files(me).unwrap().files.len());
        time("memory_map(self)", &|| {
            memory_map(me).unwrap().regions.len()
        });
        time("threads(self)", &|| threads(me).unwrap().len());
    }

    #[test]
    fn missing_process_is_reported_as_gone() {
        // PIDs above pid_max cannot exist.
        let err = open_files(u32::MAX - 1).unwrap_err();
        assert_eq!(err.kind(), crate::errors::ErrorKind::ProcessNotFound);
    }
}
