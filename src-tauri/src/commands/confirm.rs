//! A WebView boolean is not proof of a user gesture. Sensitive operations
//! require consent in an OS dialog owned by the backend, on a blocking worker.

use tauri::{AppHandle, Runtime};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tm_core::errors::{AppError, AppResult};

static DIALOG: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub fn confirm<R: Runtime>(app: &AppHandle<R>, message: &str) -> AppResult<()> {
    let _guard = DIALOG.try_lock().map_err(|_| AppError::OperationBusy)?;
    if app
        .dialog()
        .message(message)
        .title("Task Manager — confirm operation")
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancel)
        .blocking_show()
    {
        Ok(())
    } else {
        Err(AppError::OperationCancelled)
    }
}
