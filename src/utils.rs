use std::{
    borrow::Cow,
    fs::OpenOptions,
    io::{self, Write},
    path::Path,
    process::Output,
};

/// Writes/overwrites a file, assigning the permissions of `mode` if on a unix system
pub fn write_file(path: &Path, data: &[u8], _mode: u32) -> io::Result<()> {
    let mut opts = OpenOptions::new();
    opts.create(true).write(true).truncate(true);

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(_mode);
    }

    let mut file = opts.open(path)?;
    file.write_all(data)?;

    // Ensure that the data/metadata is synced and catch errors before dropping
    file.sync_all()
}

/// Warp the output of a command in a `std::io::Result` if the command failed
#[cfg(not(feature = "encoding"))]
pub fn wrap_output(output: Output) -> std::io::Result<Output> {
    if output.status.success() {
        Ok(output)
    } else {
        let mut msg = String::from_utf8_lossy(&output.stderr);
        if msg.trim().is_empty() {
            msg = String::from_utf8_lossy(&output.stdout);
        }
        if msg.is_empty() {
            msg = Cow::Borrowed("Failed to execute command with no output");
        }
        Err(io::Error::new(
            io::ErrorKind::Other,
            format!(
                "Command failed with exit code {}: {}",
                output.status.code().unwrap_or(-1),
                msg
            ),
        ))
    }
}

/// Warp the output of a command in a `std::io::Result` if the command failed
#[cfg(feature = "encoding")]
pub fn wrap_output(output: Output) -> std::io::Result<Output> {
    use encoding::decode;

    if output.status.success() {
        Ok(output)
    } else {
        let mut msg = decode(&output.stderr).unwrap_or_default();
        if msg.trim().is_empty() {
            msg = decode(&output.stdout).unwrap_or_default();
        }
        if msg.is_empty() {
            msg = Cow::Borrowed("Failed to execute command with no output");
        }
        Err(io::Error::new(
            io::ErrorKind::Other,
            format!(
                "Command failed with exit code {}: {}",
                output.status.code().unwrap_or(-1),
                msg
            ),
        ))
    }
}

#[cfg(feature = "encoding")]
pub mod encoding {
    use encoding_rs::UTF_8;
    #[cfg(windows)]
    use encoding_utils::windows::current_acp_encoding_no_replacement;
    use std::borrow::Cow;
    use std::io::{Error, ErrorKind};
    /// probe the encoding of a bytes slice, and decode it to a string(utf-8)
    pub fn decode(bytes: &[u8]) -> Result<Cow<'_, str>, Error> {
        #[cfg(windows)]
        let encoding = current_acp_encoding_no_replacement().unwrap_or(UTF_8);
        #[cfg(not(windows))]
        let encoding = &UTF_8;

        let (result, _, had_errors) = encoding.decode(bytes);
        if had_errors {
            Err(Error::new(
                ErrorKind::InvalidData,
                "Failed to decode the bytes",
            ))
        } else {
            Ok(result)
        }
    }
}

/// Safe wrappers over the POSIX calls the launchd manager makes.
#[cfg(unix)]
pub mod posix {
    use std::{
        ffi::{CStr, OsStr},
        io,
        mem::MaybeUninit,
        os::unix::ffi::OsStrExt,
        path::PathBuf,
    };

    /// The calling process's real user id.
    pub fn getuid() -> u32 {
        // SAFETY: getuid takes no arguments, cannot fail and touches no memory of ours.
        unsafe { libc::getuid() }
    }

    /// The home directory of the user with `uid`, from the password database.
    ///
    /// Fails with `NotFound` when no user has `uid` or the user has no home directory.
    pub fn home_dir(uid: u32) -> io::Result<PathBuf> {
        // getpwuid_r reports ERANGE when the entry does not fit, and the buffer doubles up to
        // the cap. Real entries are a few hundred bytes.
        const INITIAL_BUFFER_LEN: usize = 1024;
        const MAX_BUFFER_LEN: usize = 1024 * 1024;

        let mut buffer: Vec<libc::c_char> = vec![0; INITIAL_BUFFER_LEN];
        loop {
            let mut passwd = MaybeUninit::<libc::passwd>::uninit();
            let mut result: *mut libc::passwd = std::ptr::null_mut();
            // SAFETY: passwd and result are valid for writes, and buffer is valid for
            // buffer.len() bytes; getpwuid_r writes only within them.
            let errno = unsafe {
                libc::getpwuid_r(
                    uid,
                    passwd.as_mut_ptr(),
                    buffer.as_mut_ptr(),
                    buffer.len(),
                    &mut result,
                )
            };
            match errno {
                0 if result.is_null() => {
                    return Err(io::Error::new(
                        io::ErrorKind::NotFound,
                        format!("no user has uid {uid}"),
                    ));
                }
                0 => {
                    // SAFETY: a zero return with a non-null result means getpwuid_r filled
                    // the passwd it points at.
                    let dir = unsafe { (*result).pw_dir };
                    if dir.is_null() {
                        return Err(io::Error::new(
                            io::ErrorKind::NotFound,
                            format!("user {uid} has no home directory"),
                        ));
                    }
                    // SAFETY: pw_dir is a NUL-terminated string inside buffer, which is alive
                    // and unchanged until this function returns.
                    let dir = unsafe { CStr::from_ptr(dir) };
                    return Ok(PathBuf::from(OsStr::from_bytes(dir.to_bytes())));
                }
                libc::ERANGE if buffer.len() < MAX_BUFFER_LEN => {
                    buffer.resize(buffer.len() * 2, 0);
                }
                errno => return Err(io::Error::from_raw_os_error(errno)),
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn home_dir_reads_the_password_database() {
            let root_home = if cfg!(target_os = "macos") {
                "/var/root"
            } else {
                "/root"
            };
            assert_eq!(home_dir(0).unwrap(), PathBuf::from(root_home));
        }

        #[test]
        fn home_dir_of_a_uid_no_user_has_is_not_found() {
            // Far above any uid macOS or Linux gives an account: nobody is 4294967294 on macOS
            // and 65534 on Linux.
            const UNUSED_UID: u32 = 4_000_000_000;

            let err = home_dir(UNUSED_UID).unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::NotFound);
        }
    }
}

/// Stand-ins for the POSIX calls on platforms without uids or a password database.
///
/// launchd does not run on these platforms, so nothing reaches the system with these values.
#[cfg(not(unix))]
pub mod posix {
    use std::{io, path::PathBuf};

    /// `u32::MAX`, POSIX's `(uid_t)-1`, which names no user.
    pub fn getuid() -> u32 {
        u32::MAX
    }

    /// Always fails with `Unsupported`.
    pub fn home_dir(_uid: u32) -> io::Result<PathBuf> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "this platform has no password database to look home directories up in",
        ))
    }
}
