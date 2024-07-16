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
            msg = "Failed to execute command with no output".to_string();
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
    use charset_normalizer_rs::from_bytes;
    use std::io::{Error, ErrorKind};

    /// probe the encoding of a bytes slice, and decode it to a string(utf-8)
    pub fn decode(bytes: &[u8]) -> Result<String, Error> {
        let bytes = bytes.to_owned();
        let result = from_bytes(&bytes, None);
        let best_guess = result.get_best().ok_or(Error::new(
            ErrorKind::InvalidData,
            "Failed to guess encoding",
        ))?;
        Ok(best_guess
            .decoded_payload()
            .ok_or(Error::new(ErrorKind::InvalidData, "Failed to decode bytes"))?
            .to_string())
    }

    mod test {
        #[test]
        fn test_decode() {
            let hex = "9d68d55aa3acd3d6b751d6d0cec45b335da1a2ccc6d4925b345da1a2c841d55a5b355da3acd6b8d5fb82809d68d55ad7e5bbf2d5dfc6e4d55ad7e5c0efb5c4d2bbb74ed55ad1d4a1a39d68d55ad7e59ee9b7d6cef6d55ab5c4d2bbd6a7bcd2d7e5a3ac8cd99d68b2d8d55acfb5a1a39d68d55ac8e7d2959ee986ced2bbd55ad1d4a3ac9ee9cac0bde7cab9d3c3c8cb94b5d7eeb6e0b5c4d55ad1d4a3acc4bfc7b0cac0bde7d3d0cee5b7d6d6aed2bbc8cbbfdad7f69ee9c4b8d55aa1a3c6e4d3d0b6e0b74eb7d6d6a7a3acae94d6d0b9d9d492d7ee9ee9c1f7d0d0a3acc6e4d1dcc9fab6f881edb5c4ac46b4fa98cb9cca9d68d55aa3ac9ee9d6d0c841c8cbc3f1b9b2bacd87f8b5c4c6d5cda8d492a1a2d2d4bcb0d6d0c841c3f187f8b5c487f8d55aa1a3b4cbcde2a3ac9d68d55adf80cac7c293bacf87f8d5fdcabdd55acec45b365d5b335da3ac814bb1bbc9cfbaa3bacfd7f7bd4dbf97b5c887f8eb48bd4dbf9792f1d3c39ee9b9d9b7bdd55ad1d4a1a39d68d55ad4dad2d4c6e4d7f69ee9c4b8d55ab5c4b5d8b7bd95fed3d0b2bbcdacb5c4cda8b751a3acc0fdc8e7d4dac55f9eb35b375da1a2cfe3b8db5b385dbcb0b0c4e9545b395dcda8b7519ee9a1b8d6d0cec4a1b9a3acd4daf15281edcef78186bcb0d0c2bcd3c6c2cda8b7519ee9a1b8c841d55aa1b9b5c8a3a8d3c9b4cbd1dcc9fac841d55ab5c4b6a8c1788696ee7da3a95bd45d20315da1a30a0a8ca6ecb69d68d55acfc28cd9d55ad1d4b5c4b7d6ee90a3ac8c57bde7d6f7d2aad3d083c9b74ed35efc63a3acd2bbb74ed35efc638ca29d68d55ab6a8c1789ee9d55ad1d4a3ac814b8ca2b9d9d492a1a2da4dd55aa1a2e97dd55aa1a2bb9bd55aa1a2bfcdbcd2d492a1a285c7d55aa1a2cfe6d55ac6dfb4f3b7d6d6a7b6a8c1789ee9d2bbbc89b7bdd1d4a3bbc1edd2bbb74ed35efc6384748ca29d68d55ad2959ee9d55ad7e5a3acc6dfb4f3b7d6d6a7d2f29f6fb7a8bba5cfe09ccfcda8b6f8d2959ee9d55ad6a7a3acb6f8d55ad6a7cfc2c3e6b5c4b8f78280b7d6d6a7b1bbd2959ee9aa9ac1a2b5c4d55ad1d45b31305da3acc8e787f8eb4898cb9ccabbafbd4dbf97becd8ca29d68d55ad7e5b7d69ee93133b74ed55ad1d4a3bae97d967cd55aa1a29578d55aa1a2b9d9d492a1a2c6cecfc9d55aa1a2bbd5d55aa1a2e97dd6d0d55aa1a2da4dd55aa1a2bfcdbcd2d492a1a2cfe6d55aa1a2e97db1b1d55aa1a2e97dc4cfd55aa1a285c7d55aa1a2bb9bd55aa1a39f6fd59392f1d3c3c4c4b74ed35efc63a3acb8f7b74ecfc28cd9d55ad1d4d6aee967b5d8cebbc6bdb5c8a3accbfc8283b6bccac79d68d55ad165b5c4d2bbb7d6d7d3a1a39d68d55aa3a8bbf2d6d0cec4a3a9cac7cbfc8283b5c4bcafbacff377a3ac814bb7c78ca3d6b8cbfc8283ae94d6d0b5c4c6e4d6d0d2bbb74ea1a30a0a9d68d55ab5c4cec4d7d695f88c91cfb5bd79cac79d68d7d6a3acd3d6b7519d68cec4a1a2c841cec4a1a2d6d0cec4a1a2ccc6cec4a3acd4dad6d0c841c3f187f8d3d6b7519ee987f8cec4a3accac7d2bbb74ed2e2d2f4cec4d7d6a3acb1edd2e2d6aecdac9572d2b2bedfd2bbb6a8b1edd2f4b9a6c4dca1a39d68d55a8cd9b7d6cef6d55aa3acc7d2c295d57bb1e6c178a1a39d68d55ab0fcbaac95f8c3e6d55abcb0bfdad55a83c9b2bfb7d6a3acb9c5b4fa95f8c3e6d55ab7519ee9cec4d1d4cec4a3acac46b4fa95f8c3e6d55ad2bbb0e3d6b8b9d9d492b0d7d492cec4a3acbcb4cab9d3c398cb9ccab9d9d492cec4b7a8a1a2d47e8fa1b5c4d6d0cec4cda8d0d0cec4f377a1a3bb9bb8dbb0c4b5c8b5d8d3d6c1f7d0d08a41eb73cec4d1d4cec4a1a2b9d9d492b0d7d492cec4bcb0bb9bd55ab0d7d492cec4b5c4c8fdbcb0b5dacec4f377a3ac85c7d55a855ed2e0c5bcd3d0c8cb95f88c9185c7d55ab0d7d492cec4a1a39d68d7d6d2b2b1bbc6e4cbfbd55ad1d4cab9d3c3a3ac814bd0ceb3c99d68d7d6cec4bbafc8a6a1a30a";
            let bytes = hex
                .as_bytes()
                .chunks(2)
                .map(|chunk| u8::from_str_radix(std::str::from_utf8(chunk).unwrap(), 16).unwrap())
                .collect::<Vec<u8>>();
            let result = super::decode(&bytes);
            eprintln!("{:?}", result);
            assert!(result.is_ok());
            let result = result.unwrap();
            assert_eq!(&result[0..6], "漢語")
        }
    }
}
