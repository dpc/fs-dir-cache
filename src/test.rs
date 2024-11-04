use std::os::unix::ffi::OsStringExt;
use std::path::PathBuf;
use std::process::Stdio;
use std::str::FromStr;
use std::time::Duration;
use std::{ffi, fs, thread};

use anyhow::Result;
use assert_cmd::assert::OutputAssertExt as _;
use assert_cmd::cargo;

#[test]
fn sanity_check() -> Result<()> {
    let root_dir = tempfile::tempdir()?;

    thread::scope(|s| -> Result<()> {
        for _ in 0..5 {
            s.spawn(|| -> Result<()> {
                let mut cmd = our_bin_cmd();

                cmd.env("FS_DIR_CACHE_ROOT", root_dir.path());
                cmd.stderr(Stdio::inherit());
                cmd.args([
                    "lock",
                    "--key-name",
                    "keyname",
                    "--lock-id",
                    "lockid",
                    "--timeout-secs",
                    "10",
                ]);

                let dir_str = ffi::OsString::from_str(
                    String::from_utf8(
                        cmd.output()?.assert().success().get_output().stdout.clone(),
                    )?
                    .trim(),
                )?;
                let dir_path = PathBuf::from(&dir_str);
                let testfile_path = dir_path.join("test");

                fs::write(&testfile_path, [])?;
                thread::sleep(Duration::from_millis(900));
                fs::remove_file(&testfile_path)?;

                let mut cmd = our_bin_cmd();

                cmd.stderr(Stdio::inherit());
                cmd.env("FS_DIR_CACHE_ROOT", root_dir.path());
                cmd.args(["unlock", "--lock-id", "lockid"]);
                cmd.args([
                    ffi::OsString::from_vec("--dir".as_bytes().to_vec()),
                    dir_str,
                ]);
                cmd.assert().success();
                Ok(())
            });

            s.spawn(|| -> Result<()> {
                let mut cmd = our_bin_cmd();

                cmd.stderr(Stdio::inherit());
                cmd.env("FS_DIR_CACHE_ROOT", root_dir.path());
                cmd.args([
                    "exec",
                    "--key-name",
                    "keyname",
                    "--",
                    "bash",
                    "-c",
                    "set -e; test ! -e test; touch test; sleep .9; test -e test; rm test",
                ]);

                cmd.assert().success();

                Ok(())
            });
        }
        Ok(())
    })?;

    Ok(())
}

fn our_bin_cmd() -> std::process::Command {
    std::process::Command::new(cargo::cargo_bin(env!("CARGO_PKG_NAME")))
}
