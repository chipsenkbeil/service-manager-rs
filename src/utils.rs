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
