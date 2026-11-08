//! Background services. The platform layer lists them and refuses protected
//! ones; the handler adds the OS confirmation naming the real service.

use super::blocking::run as blocking;
use tauri::AppHandle;
use tm_core::errors::AppResult;
use tm_core::models::{ServiceAction, ServiceInfo, ServiceScope};
use tm_core::platform;

#[tauri::command]
pub async fn list_services(scope: ServiceScope) -> AppResult<Vec<ServiceInfo>> {
    blocking(move || platform::services(scope)).await
}

/// Start, stop or restart a service after native confirmation.
#[tauri::command]
pub async fn control_service(
    app: AppHandle,
    scope: ServiceScope,
    unit: String,
    action: ServiceAction,
) -> AppResult<()> {
    blocking(move || {
        platform::control_service(scope, &unit, action, &|service| {
            let verb = match action {
                ServiceAction::Start => "Start",
                ServiceAction::Stop => "Stop",
                ServiceAction::Restart => "Restart",
            };
            super::confirm::confirm(
                &app,
                &format!(
                    "{verb} {}?\n{}\nPrograms that depend on it may stop working.",
                    service.unit, service.description
                ),
            )
        })
    })
    .await
}
