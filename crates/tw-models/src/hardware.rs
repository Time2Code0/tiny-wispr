use sysinfo::{MemoryRefreshKind, System};
use tw_core::hotkey::Platform;
use tw_core::models::HardwareInfo;

/// What this computer can run. Reads OS counters only: no prompts, no
/// subprocesses. Memory is 0 if it can't be read.
pub fn hardware() -> HardwareInfo {
    let total_memory_bytes = System::new().map_or(0, |mut system| {
        system.refresh_memory_specifics(MemoryRefreshKind::nothing().with_ram());
        system.total_memory()
    });
    let cores = System::physical_core_count()
        .ok()
        .or_else(|| std::thread::available_parallelism().ok().map(usize::from))
        .unwrap_or(1);
    HardwareInfo {
        platform: Platform::current(),
        arch: std::env::consts::ARCH.to_owned(),
        total_memory_bytes,
        cpu_cores: u32::try_from(cores).unwrap_or(u32::MAX),
        apple_silicon: cfg!(all(target_os = "macos", target_arch = "aarch64")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_this_machine() {
        let info = hardware();
        assert!(info.total_memory_bytes > 512 * 1024 * 1024, "{info:?}");
        assert!(info.cpu_cores >= 1);
        assert_eq!(info.arch, std::env::consts::ARCH);
    }
}
