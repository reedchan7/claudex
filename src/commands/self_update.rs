//! `claudex self-update` — update claudex itself to the latest GitHub release.
//!
//! Primary path is native: resolve the latest release tag, download the
//! prebuilt tarball for this platform, verify its sha256, extract it, and
//! atomically replace the running binary. Failures *before* we hold a
//! verifiable artifact (reaching GitHub, downloading the archive) fall back to
//! the canonical `install.sh`. Once we hold the archive, any failure to verify
//! it — a checksum mismatch *or* an inability to obtain/parse the checksum — is
//! fatal and never falls back to the (unverified) install script.

use colored::Colorize;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

const REPO: &str = "reedchan7/claudex";
const USER_AGENT: &str = concat!("claudex/", env!("CARGO_PKG_VERSION"));
/// Upper bound on the release archive we'll buffer into memory. Our binaries
/// are a few MB; this just guards against a pathological response.
const MAX_DOWNLOAD: u64 = 64 * 1024 * 1024;

#[derive(Deserialize)]
struct Release {
    tag_name: String,
}

/// A native-update failure.
///
/// Once we hold a downloaded archive we install it only if we can prove it
/// authentic. Both a checksum `Checksum` mismatch and an inability to
/// obtain/parse the checksum (`Unverified`) are fatal and never fall back to
/// the unverified install script. `Recoverable` failures happen before we hold
/// a verifiable artifact and may fall back.
enum UpdateError {
    Checksum { expected: String, actual: String },
    Unverified(String),
    Recoverable(String),
}

pub async fn run(check: bool, force: bool) {
    let current = env!("CARGO_PKG_VERSION");

    let latest = match latest_version().await {
        Ok(v) => v,
        Err(e) => {
            if check {
                eprintln!("{} could not check for updates: {}", "✗".red(), e);
                std::process::exit(1);
            }
            eprintln!(
                "{} could not reach GitHub ({}); falling back to the install script",
                "!".yellow(),
                e
            );
            run_installer_fallback(None);
            return;
        }
    };

    println!("  current {}  latest {}", current.cyan(), latest.green());
    let up_to_date = current == latest;

    // `--check` reports the truth regardless of `--force`.
    if check {
        if up_to_date {
            println!("  {} already up to date", "✓".green());
        } else {
            println!(
                "  {} update available — run `claudex self-update`",
                "↑".yellow()
            );
        }
        return;
    }

    if up_to_date && !force {
        println!("  {} already up to date", "✓".green());
        return;
    }

    let Some(platform) = detect_platform() else {
        eprintln!("  self-update is not supported on this platform.");
        eprintln!("  download the latest binary from https://github.com/{REPO}/releases/latest");
        std::process::exit(1);
    };

    match native_update(&latest, &platform).await {
        Ok(()) => {
            println!(
                "  {} updated {} → {}",
                "✓".green(),
                current.dimmed(),
                latest.green()
            );
        }
        Err(UpdateError::Checksum { expected, actual }) => {
            eprintln!(
                "  {} checksum mismatch — aborting without falling back.",
                "✗".red()
            );
            eprintln!("    expected {expected}");
            eprintln!("    actual   {actual}");
            std::process::exit(1);
        }
        Err(UpdateError::Unverified(e)) => {
            eprintln!(
                "  {} could not verify the download ({e}) — aborting without falling back.",
                "✗".red()
            );
            std::process::exit(1);
        }
        Err(UpdateError::Recoverable(e)) => {
            eprintln!("  {} native update failed: {}", "!".yellow(), e);
            eprintln!("  falling back to the install script…");
            run_installer_fallback(Some(&latest));
        }
    }
}

/// Resolve the latest release version (the tag without a leading `v`).
async fn latest_version() -> Result<String, String> {
    let client = http_client()?;
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let resp = client
        .get(&url)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let release: Release = resp
        .json()
        .await
        .map_err(|e| format!("failed to parse release info: {e}"))?;
    Ok(normalize_version(&release.tag_name).to_string())
}

/// Download, verify, extract, and swap in the latest binary for `platform`.
async fn native_update(latest: &str, platform: &str) -> Result<(), UpdateError> {
    let tag = format!("v{latest}");
    let base = format!("https://github.com/{REPO}/releases/download/{tag}");
    let archive_name = format!("claudex-{tag}-{platform}.tar.gz");
    let archive_url = format!("{base}/{archive_name}");
    let sha_url = format!("{base}/claudex-{tag}-{platform}.sha256");

    let client = http_client().map_err(UpdateError::Recoverable)?;

    println!("  downloading {archive_name}…");
    let resp = get(&client, &archive_url)
        .await
        .map_err(UpdateError::Recoverable)?;
    if resp.content_length().is_some_and(|len| len > MAX_DOWNLOAD) {
        return Err(UpdateError::Recoverable(format!(
            "refusing to download more than {MAX_DOWNLOAD} bytes"
        )));
    }
    let archive = resp
        .bytes()
        .await
        .map_err(|e| UpdateError::Recoverable(format!("failed to read download: {e}")))?;

    // We now hold an archive we can't yet trust: any failure to fetch, parse,
    // or match its checksum is fatal — never launder it through the unverified
    // install script.
    let sha_body = get(&client, &sha_url)
        .await
        .map_err(|e| UpdateError::Unverified(format!("failed to fetch checksum: {e}")))?
        .text()
        .await
        .map_err(|e| UpdateError::Unverified(format!("failed to read checksum: {e}")))?;

    verify_checksum(&archive, &sha_body)?;

    // The staging dir removes itself on drop, including on the early returns above.
    let staging = StagingDir::new().map_err(UpdateError::Recoverable)?;
    let new_bin = extract_binary(staging.path(), &archive_name, &archive)
        .map_err(UpdateError::Recoverable)?;
    self_replace::self_replace(&new_bin)
        .map_err(|e| UpdateError::Recoverable(format!("failed to replace running binary: {e}")))?;
    Ok(())
}

/// Run the canonical `install.sh` via curl/wget, inheriting stdio so the user
/// sees its output. When the target version is known the script is pinned to
/// that release tag (so a moving `main` can't change what runs); otherwise it
/// uses `main`. Exits the process on failure.
fn run_installer_fallback(latest: Option<&str>) {
    let url = match latest {
        Some(v) => format!("https://raw.githubusercontent.com/{REPO}/v{v}/install.sh"),
        None => format!("https://raw.githubusercontent.com/{REPO}/main/install.sh"),
    };
    let piped = if command_exists("curl") {
        format!("curl -fsSL {url} | sh")
    } else if command_exists("wget") {
        format!("wget -qO- {url} | sh")
    } else {
        eprintln!(
            "{} need curl or wget to run the fallback installer",
            "✗".red()
        );
        std::process::exit(1);
    };

    match Command::new("sh").arg("-c").arg(&piped).status() {
        Ok(s) if s.success() => {}
        Ok(s) => std::process::exit(s.code().unwrap_or(1)),
        Err(e) => {
            eprintln!("{} failed to run the install script: {}", "✗".red(), e);
            std::process::exit(1);
        }
    }
}

/// A scratch directory that removes itself on drop, so a failed update never
/// leaves a temp dir behind.
struct StagingDir(PathBuf);

impl StagingDir {
    fn new() -> Result<Self, String> {
        let dir = std::env::temp_dir().join(format!("claudex-self-update-{}", std::process::id()));
        fs::create_dir_all(&dir).map_err(|e| format!("failed to create temp dir: {e}"))?;
        Ok(Self(dir))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for StagingDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Extract the `claudex` binary from a `.tar.gz` archive into `dir` and return
/// its path. Shells out to `tar` (always present on macOS/Linux).
fn extract_binary(dir: &Path, archive_name: &str, archive: &[u8]) -> Result<PathBuf, String> {
    let archive_path = dir.join(archive_name);
    fs::write(&archive_path, archive).map_err(|e| format!("failed to write archive: {e}"))?;

    let status = Command::new("tar")
        .arg("-xzf")
        .arg(&archive_path)
        .arg("-C")
        .arg(dir)
        .status()
        .map_err(|e| format!("failed to run tar: {e}"))?;
    if !status.success() {
        return Err("tar extraction failed".to_string());
    }

    let bin =
        find_binary(dir).ok_or_else(|| "binary 'claudex' not found in archive".to_string())?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&bin)
            .map_err(|e| format!("failed to read binary permissions: {e}"))?
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&bin, perms)
            .map_err(|e| format!("failed to set binary permissions: {e}"))?;
    }

    Ok(bin)
}

/// Recursively find a file named `claudex` under `dir`.
fn find_binary(dir: &Path) -> Option<PathBuf> {
    for entry in fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_binary(&path) {
                return Some(found);
            }
        } else if path.file_name().and_then(|n| n.to_str()) == Some("claudex") {
            return Some(path);
        }
    }
    None
}

fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| format!("failed to build HTTP client: {e}"))
}

/// GET `url` with the claudex User-Agent, erroring on non-success status.
async fn get(client: &reqwest::Client, url: &str) -> Result<reqwest::Response, String> {
    let resp = client
        .get(url)
        .header("User-Agent", USER_AGENT)
        .send()
        .await
        .map_err(|e| format!("request to {url} failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("request to {url} failed: HTTP {}", resp.status()));
    }
    Ok(resp)
}

fn command_exists(cmd: &str) -> bool {
    // `command -v` is the portable probe (matches install.sh) and avoids relying
    // on every tool supporting `--version`.
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {cmd}"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Map a Rust target (os, arch) to a release asset platform string, e.g.
/// `("macos", "aarch64") -> "darwin-arm64"`. Returns None for platforms we
/// don't self-update natively (Windows / unknown).
fn platform_for(os: &str, arch: &str) -> Option<String> {
    let os = match os {
        "macos" => "darwin",
        "linux" => "linux",
        _ => return None,
    };
    let arch = match arch {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        _ => return None,
    };
    Some(format!("{os}-{arch}"))
}

fn detect_platform() -> Option<String> {
    platform_for(std::env::consts::OS, std::env::consts::ARCH)
}

/// Strip a leading `v` from a release tag (`v1.2.3` -> `1.2.3`).
fn normalize_version(tag: &str) -> &str {
    tag.strip_prefix('v').unwrap_or(tag)
}

/// Extract the hex digest from a `*.sha256` file body of the form
/// `<64-hex>  <filename>`.
fn parse_sha256(body: &str) -> Option<String> {
    let token = body.split_whitespace().next()?;
    (token.len() == 64 && token.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| token.to_ascii_lowercase())
}

fn hex_digest(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for b in digest {
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// Verify `archive` against a `*.sha256` file body. Both an unparseable
/// checksum (`Unverified`) and a digest mismatch (`Checksum`) are fatal: once
/// we hold the archive we never fall back to the unverified install script.
fn verify_checksum(archive: &[u8], sha_body: &str) -> Result<(), UpdateError> {
    let expected = parse_sha256(sha_body)
        .ok_or_else(|| UpdateError::Unverified("could not parse checksum file".to_string()))?;
    let actual = hex_digest(archive);
    if actual != expected {
        return Err(UpdateError::Checksum { expected, actual });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_mapping_known() {
        assert_eq!(
            platform_for("macos", "aarch64").as_deref(),
            Some("darwin-arm64")
        );
        assert_eq!(
            platform_for("macos", "x86_64").as_deref(),
            Some("darwin-amd64")
        );
        assert_eq!(
            platform_for("linux", "x86_64").as_deref(),
            Some("linux-amd64")
        );
        assert_eq!(
            platform_for("linux", "aarch64").as_deref(),
            Some("linux-arm64")
        );
    }

    #[test]
    fn platform_mapping_unsupported() {
        assert_eq!(platform_for("windows", "x86_64"), None);
        assert_eq!(platform_for("macos", "riscv64"), None);
        assert_eq!(platform_for("freebsd", "x86_64"), None);
    }

    #[test]
    fn normalize_version_strips_v() {
        assert_eq!(normalize_version("v0.4.5"), "0.4.5");
        assert_eq!(normalize_version("0.4.5"), "0.4.5");
    }

    #[test]
    fn parse_sha256_real_format() {
        let body = "11b0bb21b772f0f2f61eb1155dd292809bf8e6c00dd2755bbc03bd509684721b  claudex-v0.4.5-darwin-arm64.tar.gz\n";
        assert_eq!(
            parse_sha256(body).as_deref(),
            Some("11b0bb21b772f0f2f61eb1155dd292809bf8e6c00dd2755bbc03bd509684721b")
        );
    }

    #[test]
    fn parse_sha256_rejects_garbage() {
        assert_eq!(parse_sha256(""), None);
        assert_eq!(parse_sha256("not-a-hash file"), None);
        // Too short to be a sha256.
        assert_eq!(parse_sha256("abcdef  file"), None);
    }

    #[test]
    fn hex_digest_of_empty() {
        // Well-known sha256 of the empty input.
        assert_eq!(
            hex_digest(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    // sha256("hello") — used to drive verify_checksum against a known digest.
    const HELLO_SHA256: &str = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";

    #[test]
    fn verify_checksum_accepts_matching_digest() {
        let body = format!("{HELLO_SHA256}  claudex-vX-darwin-arm64.tar.gz\n");
        assert!(verify_checksum(b"hello", &body).is_ok());
    }

    #[test]
    fn verify_checksum_rejects_mismatch_as_fatal() {
        // A valid-shaped but wrong digest must abort fatally, never fall back.
        let wrong = "f".repeat(64);
        let body = format!("{wrong}  archive.tar.gz");
        match verify_checksum(b"hello", &body) {
            Err(UpdateError::Checksum { expected, actual }) => {
                assert_eq!(expected, wrong);
                assert_eq!(actual, HELLO_SHA256);
            }
            _ => panic!("expected fatal Checksum error on digest mismatch"),
        }
    }

    #[test]
    fn verify_checksum_unparseable_is_fatal() {
        // No usable digest in the body → fatal: we won't install something we
        // can't verify, and we must not fall back to the unverified installer.
        match verify_checksum(b"hello", "garbage, no hash here") {
            Err(UpdateError::Unverified(_)) => {}
            _ => panic!("expected fatal Unverified error for unparseable checksum"),
        }
    }
}
