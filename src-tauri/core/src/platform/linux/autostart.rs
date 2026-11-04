//! Startup programs via XDG autostart (the mechanism GNOME, KDE, Xfce and the
//! rest all honour). Entries are `.desktop` files in `$XDG_CONFIG_HOME/autostart`
//! and `$XDG_CONFIG_DIRS/autostart`; a user file with the same name overrides
//! the system one, and `Hidden=true` means "treat as deleted".
//!
//! Disabling never deletes anything: it sets `Hidden=true` in the user's copy
//! (creating a small override for system entries), and enabling reverses that,
//! so the change is always undoable and system files are never touched.

use std::fs;
use std::path::{Path, PathBuf};

use crate::errors::{AppError, AppResult};
use crate::models::{StartupItem, StartupScope};

const GROUP: &str = "[Desktop Entry]";
/// Written by GNOME's own tools; honoured on read, removed when enabling.
const GNOME_KEY: &str = "X-GNOME-Autostart-enabled";

#[derive(Debug, Default, PartialEq, Eq)]
struct Entry {
    name: Option<String>,
    comment: Option<String>,
    exec: Option<String>,
    only_in: Vec<String>,
    not_show_in: Vec<String>,
    disabled: bool,
}

/// `A;B;` as a list.
fn split_list(value: &str) -> Vec<String> {
    value
        .split(';')
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Whether a desktop named in `current` (e.g. `XDG_CURRENT_DESKTOP=ubuntu:GNOME`)
/// would start an entry with these `OnlyShowIn` / `NotShowIn` lists. An unknown
/// desktop (`current` empty) is assumed to run everything.
fn runs_on(only_in: &[String], not_show_in: &[String], current: &[String]) -> bool {
    let named = |list: &[String]| {
        list.iter()
            .any(|d| current.iter().any(|c| c.eq_ignore_ascii_case(d)))
    };
    current.is_empty() || ((only_in.is_empty() || named(only_in)) && !named(not_show_in))
}

/// Reads the `[Desktop Entry]` group; everything else is ignored.
fn parse(text: &str) -> Entry {
    let mut e = Entry::default();
    let mut in_group = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            in_group = line == GROUP;
            continue;
        }
        let Some((key, value)) = line.split_once('=').filter(|_| in_group) else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "Name" => e.name = Some(value.to_owned()),
            "Comment" => e.comment = Some(value.to_owned()).filter(|v| !v.is_empty()),
            "Exec" => e.exec = Some(strip_field_codes(value)),
            "OnlyShowIn" => e.only_in = split_list(value),
            "NotShowIn" => e.not_show_in = split_list(value),
            "Hidden" if value == "true" => e.disabled = true,
            GNOME_KEY if value == "false" => e.disabled = true,
            _ => {}
        }
    }
    e
}

/// `Exec=firefox %u` is shown as `firefox`: the `%u`-style codes are
/// placeholders the launcher fills in, not part of the command.
fn strip_field_codes(exec: &str) -> String {
    let is_code = |t: &&str| t.len() == 2 && t.starts_with('%');
    exec.split_whitespace()
        .filter(|t| !is_code(t))
        .collect::<Vec<_>>()
        .join(" ")
}

/// `text` with enablement rewritten: any existing `Hidden` and GNOME keys are
/// dropped, and `Hidden=true` is added when disabling. Other lines (including
/// other groups and comments) are kept as they were.
fn with_disabled(text: &str, disabled: bool) -> String {
    let mut out = Vec::new();
    let mut in_group = false;
    let mut saw_group = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_group = t == GROUP;
            saw_group |= in_group;
            out.push(line.to_owned());
            if in_group && disabled {
                out.push("Hidden=true".to_owned());
            }
            continue;
        }
        let key = t.split_once('=').map(|(k, _)| k.trim());
        if in_group && matches!(key, Some("Hidden" | GNOME_KEY)) {
            continue;
        }
        out.push(line.to_owned());
    }
    if !saw_group && disabled {
        out.insert(0, "Hidden=true".to_owned());
        out.insert(0, GROUP.to_owned());
    }
    out.join("\n") + "\n"
}

pub struct Dirs {
    user: PathBuf,
    system: Vec<PathBuf>,
    /// The running desktop(s), from `XDG_CURRENT_DESKTOP`; empty when unknown.
    desktops: Vec<String>,
}

impl Dirs {
    pub fn from_env() -> Self {
        let var = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty());
        let config = var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| var("HOME").map(|h| Path::new(&h).join(".config")))
            .unwrap_or_default();
        let system = var("XDG_CONFIG_DIRS")
            .map(|v| std::env::split_paths(&v).collect())
            .unwrap_or_else(|| vec![PathBuf::from("/etc/xdg")]);
        Self {
            user: config.join("autostart"),
            system: system.into_iter().map(|d| d.join("autostart")).collect(),
            desktops: var("XDG_CURRENT_DESKTOP")
                .map(|v| v.to_string_lossy().split(':').map(str::to_owned).collect())
                .unwrap_or_default(),
        }
    }
}

fn read_dir(dir: &Path) -> Vec<(String, String)> {
    let Ok(rd) = fs::read_dir(dir) else {
        return Vec::new();
    };
    rd.take(2_000)
        .flatten()
        .filter_map(|f| {
            let id = f.file_name().into_string().ok()?;
            if !id.ends_with(".desktop") {
                return None;
            }
            let text = crate::filesystem::read_text(&f.path(), 256 * 1024).ok()?;
            Some((id, text))
        })
        .collect()
}

pub fn list(dirs: &Dirs) -> Vec<StartupItem> {
    use std::collections::BTreeMap;
    // Earlier directories win, as the spec says; the user's comes first.
    let mut found: BTreeMap<String, (StartupScope, Entry, bool)> = BTreeMap::new();
    let ordered = std::iter::once((&dirs.user, StartupScope::User))
        .chain(dirs.system.iter().map(|d| (d, StartupScope::System)));
    for (dir, scope) in ordered {
        for (id, text) in read_dir(dir) {
            let entry = parse(&text);
            match found.get_mut(&id) {
                None => {
                    found.insert(id, (scope, entry, false));
                }
                // A user override with no command (ours): fill in what the
                // system entry knows, keep the user's enabled state.
                Some((_, mine, filled)) if mine.exec.is_none() && !*filled => {
                    mine.name = mine.name.take().or(entry.name);
                    mine.comment = entry.comment;
                    mine.exec = entry.exec;
                    mine.only_in = entry.only_in;
                    mine.not_show_in = entry.not_show_in;
                    *filled = true;
                }
                Some(_) => {}
            }
        }
    }
    let mut items: Vec<StartupItem> = found
        .into_iter()
        .filter_map(|(id, (scope, e, filled))| {
            let command = e.exec?;
            // Listed as the system's when its definition came from there.
            let scope = if filled { StartupScope::System } else { scope };
            Some(StartupItem {
                name: e
                    .name
                    .unwrap_or_else(|| id.trim_end_matches(".desktop").to_owned()),
                id,
                comment: e.comment,
                command,
                scope,
                enabled: !e.disabled,
                runs_here: runs_on(&e.only_in, &e.not_show_in, &dirs.desktops),
                only_in: Some(e.only_in.join(", ")).filter(|v| !v.is_empty()),
            })
        })
        .collect();
    items.sort_by_key(|i| i.name.to_lowercase());
    items
}

/// `id` becomes a file name under the autostart directory, so it must be one.
fn check_id(id: &str) -> AppResult<()> {
    let ok = id.ends_with(".desktop")
        && id.len() <= 255
        && !id.starts_with('.')
        && !id.contains(['/', '\\', '\0']);
    ok.then_some(())
        .ok_or_else(|| AppError::InvalidArgument("That is not a valid startup entry.".into()))
}

fn io_err(what: &str, e: std::io::Error) -> AppError {
    match e.kind() {
        std::io::ErrorKind::PermissionDenied => {
            AppError::PermissionDenied(format!("Permission denied while {what}."))
        }
        _ => AppError::SystemQueryFailed(format!("could not {what}: {e}")),
    }
}

pub fn set_enabled(dirs: &Dirs, id: &str, enabled: bool) -> AppResult<()> {
    check_id(id)?;
    if !dirs.user.is_absolute() {
        return Err(AppError::InvalidArgument(
            "The startup directory must be absolute.".into(),
        ));
    }
    let known = list(dirs)
        .into_iter()
        .find(|i| i.id == id)
        .ok_or_else(|| AppError::InvalidArgument("That startup entry no longer exists.".into()))?;
    let path = dirs.user.join(id);
    let existing = match crate::filesystem::read_text(&path, 256 * 1024) {
        Ok(text) => Some(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(io_err("reading the startup entry", e)),
    };

    let text = match (&existing, enabled) {
        (Some(t), _) => with_disabled(t, !enabled),
        (None, true) => return Ok(()), // enabled via a system file already
        (None, false) => format!(
            "{GROUP}\nType=Application\nName={}\nHidden=true\n",
            known.name
        ),
    };
    // A user file that only existed to hide a system entry is removed on
    // enable, so the system entry applies again unchanged.
    if enabled && parse(&text).exec.is_none() {
        return fs::remove_file(&path).map_err(|e| io_err("removing the override", e));
    }
    crate::filesystem::write_private_atomic(&path, text.as_bytes())
        .map_err(|e| io_err("saving the startup entry", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    const APP: &str = "[Desktop Entry]\nType=Application\nName=Dropbox\nExec=dropbox start %u\n";

    #[test]
    fn parses_and_strips_field_codes() {
        let e = parse(&format!(
            "{APP}OnlyShowIn=GNOME;KDE;\n[Other]\nHidden=true\n"
        ));
        assert_eq!(e.exec.as_deref(), Some("dropbox start"));
        assert_eq!(e.only_in, ["GNOME", "KDE"]);
        assert!(!e.disabled, "keys of other groups are ignored");
        assert!(parse(&format!("{APP}{GNOME_KEY}=false\n")).disabled);
    }

    #[test]
    fn decides_whether_this_desktop_runs_an_entry() {
        let l = |s: &str| split_list(s);
        let gnome = vec!["ubuntu".to_owned(), "GNOME".to_owned()];
        assert!(runs_on(&l("GNOME;MATE;"), &[], &gnome), "listed desktop");
        assert!(!runs_on(&l("KDE;"), &[], &gnome), "KDE-only entry on GNOME");
        assert!(
            !runs_on(&[], &l("GNOME;"), &gnome),
            "NotShowIn names this desktop"
        );
        assert!(runs_on(&l("gnome;"), &[], &gnome), "case does not matter");
        assert!(
            runs_on(&[], &[], &gnome) && runs_on(&l("KDE;"), &[], &[]),
            "no limit / unknown desktop"
        );
    }

    #[test]
    fn rewrites_enablement_without_losing_lines() {
        let off = with_disabled(&format!("{APP}{GNOME_KEY}=true\n# keep\n"), true);
        assert!(parse(&off).disabled && off.contains("# keep") && !off.contains(GNOME_KEY));
        assert_eq!(with_disabled(&off, false), format!("{APP}# keep\n"));
        assert!(parse(&with_disabled("", true)).disabled);
    }

    #[test]
    fn rejects_ids_that_are_not_file_names() {
        for bad in ["../x.desktop", "a/b.desktop", ".hid.desktop", "x.txt", ""] {
            assert!(check_id(bad).is_err(), "{bad}");
        }
        assert!(check_id("dropbox.desktop").is_ok());
    }

    #[test]
    fn a_planted_temporary_symlink_cannot_overwrite_another_file() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let dirs = Dirs {
            user: root.path().join("user"),
            system: Vec::new(),
            desktops: vec![],
        };
        fs::create_dir(&dirs.user).unwrap();
        fs::write(dirs.user.join("app.desktop"), APP).unwrap();
        let keep = root.path().join("unrelated");
        fs::write(&keep, "keep me").unwrap();
        symlink(&keep, dirs.user.join(".app.desktop.tmp")).unwrap();
        set_enabled(&dirs, "app.desktop", false).unwrap();
        assert_eq!(fs::read_to_string(&keep).unwrap(), "keep me");
        assert!(parse(&fs::read_to_string(dirs.user.join("app.desktop")).unwrap()).disabled);
        fs::remove_file(dirs.user.join("app.desktop")).unwrap();
        symlink(&keep, dirs.user.join("app.desktop")).unwrap();
        assert!(set_enabled(&dirs, "app.desktop", true).is_err());
        assert_eq!(fs::read_to_string(keep).unwrap(), "keep me");
    }

    #[test]
    fn disables_and_re_enables_a_system_entry_through_an_override() {
        let root = std::env::temp_dir().join(format!("tm-autostart-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let dirs = Dirs {
            user: root.join("user"),
            system: vec![root.join("sys")],
            desktops: vec![],
        };
        fs::create_dir_all(&dirs.system[0]).unwrap();
        fs::write(dirs.system[0].join("dropbox.desktop"), APP).unwrap();

        let item = || list(&dirs).pop().unwrap();
        assert!(item().enabled && item().scope == StartupScope::System);

        set_enabled(&dirs, "dropbox.desktop", false).unwrap();
        let off = item();
        assert!(!off.enabled && off.command == "dropbox start" && off.name == "Dropbox");
        assert_eq!(
            fs::read_to_string(dirs.system[0].join("dropbox.desktop")).unwrap(),
            APP
        );

        set_enabled(&dirs, "dropbox.desktop", true).unwrap();
        assert!(item().enabled && !dirs.user.join("dropbox.desktop").exists());
        assert!(set_enabled(&dirs, "missing.desktop", false).is_err());
        let _ = fs::remove_dir_all(&root);
    }
}
