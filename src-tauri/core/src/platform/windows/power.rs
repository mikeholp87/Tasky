//! AC/battery state via `GetSystemPowerStatus`.

use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};

use crate::benchmarks::models::{PowerSource, PowerState};

/// `BatteryFlag` bit meaning "no system battery".
const NO_BATTERY: u8 = 128;

pub fn power_state() -> PowerState {
    let mut status = SYSTEM_POWER_STATUS::default();
    // SAFETY: `status` is a valid, writable SYSTEM_POWER_STATUS.
    if unsafe { GetSystemPowerStatus(&mut status) }.is_err() {
        return PowerState::default();
    }
    let has_battery = status.BatteryFlag != 255 && status.BatteryFlag & NO_BATTERY == 0;
    let source = match status.ACLineStatus {
        1 => PowerSource::Ac,
        0 if has_battery => PowerSource::Battery,
        _ => PowerSource::Unknown,
    };
    let battery_percent = (has_battery && status.BatteryLifePercent <= 100)
        .then_some(f32::from(status.BatteryLifePercent));
    PowerState {
        source,
        battery_percent,
    }
}
