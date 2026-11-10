//! A WebView boolean is not proof of a user gesture. Sensitive operations
//! require consent in an OS dialog owned by the backend, on a blocking worker.

use tauri::{AppHandle, Runtime};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tm_core::errors::{AppError, AppResult};

pub fn confirm<R: Runtime>(app: &AppHandle<R>, message: &str) -> AppResult<()> {
    let _slot = super::blocking::dialog_slot()?;
    if app
        .dialog()
        .message(message)
        .title("Tasky — confirm operation")
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancel)
        .blocking_show()
    {
        Ok(())
    } else {
        Err(AppError::OperationCancelled)
    }
}
