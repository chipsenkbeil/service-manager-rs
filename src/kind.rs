use cfg_if::cfg_if;
use std::io;

/// Represents the kind of service manager
#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq)]
#[cfg_attr(feature = "clap", derive(::clap::ValueEnum))]
#[cfg_attr(feature = "serde", derive(::serde::Serialize, ::serde::Deserialize))]
#[cfg_attr(feature = "clap", clap(rename_all = "lowercase"))]
#[cfg_attr(feature = "serde", serde(rename_all = "lowercase"))]
pub enum ServiceManagerKind {
    /// Use launchd to manage the service
    Launchd,

    /// Use OpenRC to manage the service
    OpenRc,

    /// Use rc.d to manage the service
    Rcd,

    /// Use Windows service controller to manage the service
    Sc,

    /// Use systemd to manage the service
    Systemd,

    /// Use WinSW to manage the service
    WinSw,
}

impl ServiceManagerKind {
    /// Looks up the kind of service management platform native to the operating system
    pub fn native() -> io::Result<Self> {
        cfg_if! {
            if #[cfg(target_os = "macos")] {
                Ok(Self::Launchd)
            } else if #[cfg(target_os = "windows")] {
                use super::{ServiceManager, TypedServiceManager};

                // Prefer WinSW over sc.exe, because if it's present, it's likely been explicitly
                // installed as an alternative to sc.exe.
                let manager = TypedServiceManager::target(ServiceManagerKind::WinSw);
                if let Ok(true) = manager.available() {
                    return Ok(ServiceManagerKind::WinSw);
                }

                Ok(ServiceManagerKind::Sc)
            } else if #[cfg(any(
                target_os = "freebsd",
                target_os = "dragonfly",
                target_os = "openbsd",
                target_os = "netbsd"
            ))] {
                Ok(ServiceManagerKind::Rcd)
            } else if #[cfg(target_os = "linux")] {
                use super::{ServiceManager, TypedServiceManager};

                let manager = TypedServiceManager::target(ServiceManagerKind::Systemd);
                if let Ok(true) = manager.available() {
                    return Ok(ServiceManagerKind::Systemd);
                }

                let manager = TypedServiceManager::target(ServiceManagerKind::OpenRc);
                if let Ok(true) = manager.available() {
                    return Ok(ServiceManagerKind::OpenRc);
                }

                Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "Only systemd and openrc are supported on Linux",
                ))
            } else {
                Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "Service manager are not supported on current Operating System!",
                ))
            }
        }
    }
}
