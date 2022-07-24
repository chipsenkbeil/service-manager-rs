use std::{
    fs::OpenOptions,
    io::{self, Write},
    path::Path,
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
