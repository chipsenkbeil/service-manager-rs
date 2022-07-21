/// Represents the kind of service manager
#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq)]
#[cfg_attr(feature = "clap", derive(::clap::ValueEnum))]
#[cfg_attr(feature = "serde", derive(::serde::Serialize, ::serde::Deserialize))]
#[cfg_attr(feature = "clap", clap(rename_all = "lowercase"))]
#[cfg_attr(feature = "serde", serde(rename_all = "lowercase"))]
pub enum ServiceManagerKind {
    /// Use launchd to manage the service
    #[cfg(target_os = "macos")]
    Launchd,

    /// Use OpenRC to manage the service
    #[cfg(unix)]
    OpenRc,

    /// Use rc.d to manage the service
    #[cfg(unix)]
    Rcd,

    /// Use Windows service controller to manage the service
    #[cfg(windows)]
    Sc,

    /// Use systemd to manage the service
    #[cfg(unix)]
    Systemd,
}
