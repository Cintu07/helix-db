use assert_cmd::Command;
use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use tempfile::{Builder, TempDir};

pub struct CliFixture {
    root: PathBuf,
    _tempdir: Option<TempDir>,
    home: PathBuf,
    helix_home: PathBuf,
    cache: PathBuf,
    bin: PathBuf,
}

impl CliFixture {
    pub fn new() -> Self {
        let root = Builder::new()
            .prefix("helix-cli-e2e-")
            .tempdir()
            .expect("create e2e tempdir");
        let root_path = root.path().to_path_buf();
        let home = root_path.join("home");
        let helix_home = root_path.join("helix-home");
        let cache = root_path.join("helix-cache");
        let bin = root_path.join("bin");
        fs::create_dir_all(&home).expect("create isolated home");
        fs::create_dir_all(&helix_home).expect("create isolated helix home");
        fs::create_dir_all(&cache).expect("create isolated cache");
        install_fake_docker(&bin);

        let tempdir = if std::env::var_os("HELIX_E2E_KEEP_TMP").is_some() {
            std::mem::forget(root);
            None
        } else {
            Some(root)
        };

        Self {
            root: root_path,
            _tempdir: tempdir,
            home,
            helix_home,
            cache,
            bin,
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn command(&self) -> Command {
        let mut command = Command::cargo_bin("helix").expect("helix binary should be built");
        let path = prepend_path(&self.bin);
        command
            .env("HELIX_NO_UPDATE_CHECK", "1")
            .env("HELIX_DISABLE_UPDATE_CHECK", "1")
            .env("NO_COLOR", "1")
            .env("HELIX_HOME", &self.helix_home)
            .env("HELIX_CACHE_DIR", &self.cache)
            .env("HOME", &self.home)
            .env("USERPROFILE", &self.home)
            .env("PATH", path)
            .env("PATHEXT", ".COM;.EXE;.BAT;.CMD")
            .env("CLICOLOR", "0");
        command
    }
}

fn prepend_path(bin: &Path) -> std::ffi::OsString {
    let mut paths = vec![bin.to_path_buf()];
    if let Some(path) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&path));
    }
    std::env::join_paths(paths).expect("join PATH")
}

fn install_fake_docker(bin: &Path) {
    fs::create_dir_all(bin).expect("create fake docker bin");

    #[cfg(windows)]
    {
        let script = bin.join("docker.cmd");
        fs::write(
            &script,
            r#"@echo off
if "%1"=="info" exit /b 0
if "%1"=="ps" exit /b 0
if "%1"=="logs" (
  echo fake logs
  exit /b 0
)
if "%1"=="rm" (
  echo No such container 1>&2
  exit /b 1
)
if "%1"=="network" (
  echo not found 1>&2
  exit /b 1
)
if "%1"=="volume" (
  echo not found 1>&2
  exit /b 1
)
exit /b 0
"#,
        )
        .expect("write fake docker cmd");
    }

    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;

        let script = bin.join("docker");
        fs::write(
            &script,
            r#"#!/bin/sh
case "$1" in
  info) exit 0 ;;
  ps) exit 0 ;;
  logs) echo "fake logs"; exit 0 ;;
  rm) echo "No such container" >&2; exit 1 ;;
  network|volume) echo "not found" >&2; exit 1 ;;
  *) exit 0 ;;
esac
"#,
        )
        .expect("write fake docker script");
        let mut permissions = fs::metadata(&script).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script, permissions).unwrap();
    }
}

/// Pick a port the OS currently considers free.
///
/// There is an unavoidable TOCTOU window here: we release the listener before
/// the caller can bind, and the caller hands the port to a Docker container in
/// a child process, so we cannot keep the `fd` alive and pass it through (the
/// usual `from_std` / `SO_REUSEPORT` mitigations don't cross the container
/// boundary). This is acceptable for these e2e tests because they are
/// `#[ignore]`d behind Docker and are not run concurrently in CI; collisions
/// would surface as a loud `helix start` failure rather than silent corruption.
pub fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind free TCP port");
    let port = listener.local_addr().expect("read local addr").port();
    drop(listener);
    port
}
