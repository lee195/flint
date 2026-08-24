//! Hardware probe (doc 01): chip identity + generation and physical RAM via `libc`
//! `sysctlbyname` — no third-party detection library, no network, no subprocess.
//! Apple Silicon is the supported target; unified memory makes RAM the dominant sizing
//! factor, which is what makes the simple tier table honest.

use std::ffi::CString;

use crate::error::AppError;
use crate::types::{ProbeResult, Tier};

/// Probe the machine. Never fails hard — a probe error is surfaced as unsupported with a
/// plain-language reason rather than blocking launch.
pub fn probe() -> Result<ProbeResult, AppError> {
    let chip = sysctl_string("machdep.cpu.brand_string")?;
    let ram_bytes = sysctl_value::<u64>("hw.memsize")?;
    let arm64 = sysctl_value::<u32>("hw.optional.arm64").unwrap_or(0) == 1;
    let ram_gb = ram_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    let tier = Tier::of_ram_gb(ram_gb);
    Ok(ProbeResult {
        chip,
        ram_gb: (ram_gb * 10.0).round() / 10.0,
        tier,
        supported: arm64,
        unsupported_reason: if arm64 {
            None
        } else {
            Some("Flint needs Apple Silicon (M-series).".to_string())
        },
    })
}

fn sysctl_string(name: &str) -> Result<String, AppError> {
    let cname = CString::new(name).map_err(|_| AppError::Config(format!("bad sysctl name: {name}")))?;
    let mut size = 0usize;
    let r = unsafe {
        libc::sysctlbyname(
            cname.as_ptr(),
            std::ptr::null_mut(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if r != 0 {
        return Err(sysctl_err(name));
    }
    let mut buf = vec![0u8; size.max(1)];
    let r = unsafe {
        libc::sysctlbyname(
            cname.as_ptr(),
            buf.as_mut_ptr() as *mut libc::c_void,
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if r != 0 {
        return Err(sysctl_err(name));
    }
    let s = String::from_utf8_lossy(&buf);
    Ok(s.trim_end_matches('\0').to_string())
}

fn sysctl_value<T: Default>(name: &str) -> Result<T, AppError> {
    let cname = CString::new(name).map_err(|_| AppError::Config(format!("bad sysctl name: {name}")))?;
    let mut val = T::default();
    let mut size = std::mem::size_of::<T>();
    let r = unsafe {
        libc::sysctlbyname(
            cname.as_ptr(),
            &mut val as *mut T as *mut libc::c_void,
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if r != 0 {
        return Err(sysctl_err(name));
    }
    Ok(val)
}

fn sysctl_err(name: &str) -> AppError {
    AppError::Config(format!("sysctlbyname({name}) failed: {}", std::io::Error::last_os_error()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tier_of_ram_boundaries() {
        assert_eq!(Tier::of_ram_gb(4.0), None); // below the floor — no tier
        assert_eq!(Tier::of_ram_gb(8.0), Some(Tier::Gb8));
        assert_eq!(Tier::of_ram_gb(11.9), Some(Tier::Gb8));
        assert_eq!(Tier::of_ram_gb(12.0), Some(Tier::Gb16));
        assert_eq!(Tier::of_ram_gb(16.0), Some(Tier::Gb16));
        assert_eq!(Tier::of_ram_gb(23.9), Some(Tier::Gb16));
        assert_eq!(Tier::of_ram_gb(24.0), Some(Tier::Gb32));
        assert_eq!(Tier::of_ram_gb(47.9), Some(Tier::Gb32));
        assert_eq!(Tier::of_ram_gb(48.0), Some(Tier::Gb64));
        assert_eq!(Tier::of_ram_gb(64.0), Some(Tier::Gb64));
        assert_eq!(Tier::of_ram_gb(128.0), Some(Tier::Gb64));
    }

    #[test]
    fn probe_on_this_machine_is_supported() {
        // The probe runs on macOS in CI/dev; this is a smoke test that it parses.
        let p = probe().expect("probe should not fail on a supported mac");
        assert!(p.supported, "expected Apple Silicon: {p:?}");
        assert!(p.ram_gb >= 8.0, "expected at least 8 GB RAM: {p:?}");
        assert!(p.tier.is_some());
    }
}
