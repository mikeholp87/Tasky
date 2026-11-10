//! Running system command-line tools (no shell) and turning their failures
//! into plain-language errors.

use std::process::Command;

use crate::errors::{AppError, AppResult};

/// Runs `command` and returns its standard output. `missing` is the error
/// when the tool is not installed; `failed` classifies the first line of
/// standard error when it exits unsuccessfully.
pub fn run_tool(
    command: &mut Command,
    missing: impl FnOnce() -> AppError,
    failed: impl FnOnce(&str) -> AppError,
) -> AppResult<String> {
    let output = command.env("LC_ALL", "C").output().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            missing()
        } else {
            AppError::SystemQueryFailed(format!(
                "Could not run {}: {e}",
                command.get_program().to_string_lossy()
            ))
        }
    })?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(failed(stderr.lines().next().unwrap_or("").trim()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_missing_tools_and_failures() {
        let missing = run_tool(
            &mut Command::new("tasky-no-such-tool"),
            || AppError::ActionUnavailable("missing".into()),
            |_| unreachable!(),
        );
        assert!(matches!(missing, Err(AppError::ActionUnavailable(m)) if m == "missing"));

        let failed = run_tool(
            Command::new("sh").args(["-c", "echo 'first line' >&2; echo second >&2; exit 3"]),
            || unreachable!(),
            |first| AppError::SystemQueryFailed(first.to_owned()),
        );
        assert!(matches!(failed, Err(AppError::SystemQueryFailed(m)) if m == "first line"));

        let ok = run_tool(
            Command::new("echo").arg("hi"),
            || unreachable!(),
            |_| unreachable!(),
        );
        assert_eq!(ok.unwrap(), "hi\n");
    }
}
