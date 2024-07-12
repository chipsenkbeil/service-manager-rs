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
    opts.create(true).write(true);

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
