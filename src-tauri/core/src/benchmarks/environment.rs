//! Preflight: facts about the machine and its state when a run starts, and
//! the warnings they imply. Warnings inform; they never block or change the
//! system (no governor, priority or power-profile changes).

use super::model::{Environment, PowerSource};
use crate::models::StaticSystemInfo;
use crate::platform;

/// System-wide CPU use above this before a run triggers a warning.
pub const BACKGROUND_CPU_WARN_PERCENT: f32 = 25.0;

pub fn collect(info: &StaticSystemInfo, background_cpu_percent: Option<f32>) -> Environment {
    let virtualization = info
        .container
        .clone()
        .or_else(|| platform::is_virtualized().then(|| "virtual machine".to_owned()));
    Environment {
        platform: info.platform,
        distribution: info.distribution.clone().or_else(|| info.os_name.clone()),
        kernel: info.kernel_version.clone(),
        architecture: info.architecture.clone(),
        cpu_model: info.cpu_brand.clone(),
        physical_cores: info.physical_core_count,
        logical_cores: info.logical_core_count,
        total_memory_bytes: info.total_memory_bytes,
        power_source: platform::power_source(),
        cpu_governor: platform::cpu_governor(),
        virtualization,
        background_cpu_percent,
        development_build: cfg!(debug_assertions),
    }
}

pub fn warnings(env: &Environment) -> Vec<String> {
    let mut out = Vec::new();
    if env.development_build {
        out.push(
            "Development build: benchmark scores are not representative and are not recorded."
                .to_owned(),
        );
    }
    if env.power_source == PowerSource::Battery {
        out.push("Battery power may reduce benchmark performance.".to_owned());
    }
    if let Some(v) = &env.virtualization {
        out.push(format!(
            "Running in a {v}. Results may reflect its resource limits rather than the host's."
        ));
    }
    if let Some(load) = env.background_cpu_percent
        && load > BACKGROUND_CPU_WARN_PERCENT
    {
        out.push(format!(
            "Background CPU activity detected ({load:.0}%). Other applications may affect accuracy."
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Platform;

    fn env() -> Environment {
        Environment {
            platform: Platform::Linux,
            distribution: Some("Debian GNU/Linux".into()),
            kernel: None,
            architecture: "x86_64".into(),
            cpu_model: "Test CPU".into(),
            physical_cores: Some(4),
            logical_cores: 8,
            total_memory_bytes: 16 << 30,
            power_source: PowerSource::Ac,
            cpu_governor: None,
            virtualization: None,
            background_cpu_percent: Some(3.0),
            development_build: false,
        }
    }

    #[test]
    fn a_clean_machine_has_no_warnings() {
        assert!(warnings(&env()).is_empty());
    }

    #[test]
    fn each_condition_produces_its_own_warning() {
        let mut e = env();
        e.power_source = PowerSource::Battery;
        e.virtualization = Some("Docker".into());
        e.background_cpu_percent = Some(60.0);
        e.development_build = true;
        let w = warnings(&e);
        assert_eq!(w.len(), 4);
        assert!(w.iter().any(|m| m.contains("Battery")));
        assert!(w.iter().any(|m| m.contains("Docker")));
        assert!(w.iter().any(|m| m.contains("60%")));
        assert!(w.iter().any(|m| m.contains("Development build")));
    }

    #[test]
    fn unknown_power_and_unknown_load_do_not_warn() {
        let mut e = env();
        e.power_source = PowerSource::Unknown;
        e.background_cpu_percent = None;
        assert!(warnings(&e).is_empty());
        // Exactly at the threshold is not "above" it.
        e.background_cpu_percent = Some(BACKGROUND_CPU_WARN_PERCENT);
        assert!(warnings(&e).is_empty());
    }
}
