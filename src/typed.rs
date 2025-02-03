use super::{
    LaunchdServiceManager, OpenRcServiceManager, RcdServiceManager, ScServiceManager,
    ScmServiceManager, ServiceInstallCtx, ServiceLevel, ServiceManager, ServiceManagerKind,
    ServiceStartCtx, ServiceStopCtx, ServiceUninstallCtx, SystemdServiceManager,
    WinSwServiceManager,
};
use std::io;

/// Represents an implementation of a known [`ServiceManager`]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypedServiceManager {
    Launchd(LaunchdServiceManager),
    OpenRc(OpenRcServiceManager),
    Rcd(RcdServiceManager),
    Sc(ScServiceManager),
    Scm(ScmServiceManager),
    Systemd(SystemdServiceManager),
    WinSw(WinSwServiceManager),
}

macro_rules! using {
    ($self:ident, $this:ident -> $expr:expr) => {{
        match $self {
            TypedServiceManager::Launchd($this) => $expr,
            TypedServiceManager::OpenRc($this) => $expr,
            TypedServiceManager::Rcd($this) => $expr,
            TypedServiceManager::Sc($this) => $expr,
            TypedServiceManager::Systemd($this) => $expr,
            TypedServiceManager::WinSw($this) => $expr,
            TypedServiceManager::Scm($this) => $expr,
        }
    }};
}

impl ServiceManager for TypedServiceManager {
    fn available(&self) -> io::Result<bool> {
        using!(self, x -> x.available())
    }

    fn install(&self, ctx: ServiceInstallCtx) -> io::Result<()> {
        using!(self, x -> x.install(ctx))
    }

    fn uninstall(&self, ctx: ServiceUninstallCtx) -> io::Result<()> {
        using!(self, x -> x.uninstall(ctx))
    }

    fn start(&self, ctx: ServiceStartCtx) -> io::Result<()> {
        using!(self, x -> x.start(ctx))
    }

    fn stop(&self, ctx: ServiceStopCtx) -> io::Result<()> {
        using!(self, x -> x.stop(ctx))
    }

    fn level(&self) -> ServiceLevel {
        using!(self, x -> x.level())
    }

    fn set_level(&mut self, level: ServiceLevel) -> io::Result<()> {
        using!(self, x -> x.set_level(level))
    }

    fn status(&self, ctx: crate::ServiceStatusCtx) -> io::Result<crate::ServiceStatus> {
        using!(self, x -> x.status(ctx))
    }
}

impl TypedServiceManager {
    /// Creates a new service using the specified type, falling back to selecting
    /// based on native service manager for the current operating system if no type provided
    pub fn target_or_native(kind: impl Into<Option<ServiceManagerKind>>) -> io::Result<Self> {
        match kind.into() {
            Some(kind) => Ok(Self::target(kind)),
            None => Self::native(),
        }
    }

    /// Creates a new service manager targeting the specific service manager kind using the
    /// default service manager instance
    pub fn target(kind: ServiceManagerKind) -> Self {
        match kind {
            ServiceManagerKind::Launchd => Self::Launchd(LaunchdServiceManager::default()),
            ServiceManagerKind::OpenRc => Self::OpenRc(OpenRcServiceManager::default()),
            ServiceManagerKind::Rcd => Self::Rcd(RcdServiceManager::default()),
            ServiceManagerKind::Sc => Self::Sc(ScServiceManager::default()),
            ServiceManagerKind::Scm => Self::Scm(ScmServiceManager::default()),
            ServiceManagerKind::Systemd => Self::Systemd(SystemdServiceManager::default()),
            ServiceManagerKind::WinSw => Self::WinSw(WinSwServiceManager::default()),
        }
    }

    /// Attempts to select the native service manager for the current operating system
    ///
    /// * For MacOS, this will use [`LaunchdServiceManager`]
    /// * For Windows, this will use [`ScServiceManager`]
    /// * For BSD variants, this will use [`RcdServiceManager`]
    /// * For Linux variants, this will use either [`SystemdServiceManager`] or [`OpenRcServiceManager`]
    pub fn native() -> io::Result<Self> {
        Ok(Self::target(ServiceManagerKind::native()?))
    }

    /// Consumes underlying [`ServiceManager`] and moves it onto the heap
    pub fn into_box(self) -> Box<dyn ServiceManager> {
        using!(self, x -> Box::new(x))
    }

    /// Returns true if [`ServiceManager`] instance is for `launchd`
    pub fn is_launchd(&self) -> bool {
        matches!(self, Self::Launchd(_))
    }

    /// Returns true if [`ServiceManager`] instance is for `OpenRC`
    pub fn is_openrc(&self) -> bool {
        matches!(self, Self::OpenRc(_))
    }

    /// Returns true if [`ServiceManager`] instance is for `rc.d`
    pub fn is_rc_d(&self) -> bool {
        matches!(self, Self::Rcd(_))
    }

    /// Returns true if [`ServiceManager`] instance is for `sc`
    pub fn is_sc(&self) -> bool {
        matches!(self, Self::Sc(_))
    }

    /// Returns true if [`ServiceManager`] instance is for `scm`
    pub fn is_scm(&self) -> bool {
        matches!(self, Self::Scm(_))
    }

    /// Returns true if [`ServiceManager`] instance is for `systemd`
    pub fn is_systemd(&self) -> bool {
        matches!(self, Self::Systemd(_))
    }

    /// Returns true if [`ServiceManager`] instance is for `winsw`
    pub fn is_winsw(&self) -> bool {
        matches!(self, Self::WinSw(_))
    }
}

impl From<super::LaunchdServiceManager> for TypedServiceManager {
    fn from(manager: super::LaunchdServiceManager) -> Self {
        Self::Launchd(manager)
    }
}

impl From<super::OpenRcServiceManager> for TypedServiceManager {
    fn from(manager: super::OpenRcServiceManager) -> Self {
        Self::OpenRc(manager)
    }
}

impl From<super::RcdServiceManager> for TypedServiceManager {
    fn from(manager: super::RcdServiceManager) -> Self {
        Self::Rcd(manager)
    }
}

impl From<super::ScServiceManager> for TypedServiceManager {
    fn from(manager: super::ScServiceManager) -> Self {
        Self::Sc(manager)
    }
}

impl From<super::ScmServiceManager> for TypedServiceManager {
    fn from(manager: super::ScmServiceManager) -> Self {
        Self::Scm(manager)
    }
}

impl From<super::SystemdServiceManager> for TypedServiceManager {
    fn from(manager: super::SystemdServiceManager) -> Self {
        Self::Systemd(manager)
    }
}

impl From<super::WinSwServiceManager> for TypedServiceManager {
    fn from(manager: super::WinSwServiceManager) -> Self {
        Self::WinSw(manager)
    }
}
