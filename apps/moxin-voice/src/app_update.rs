use serde::Deserialize;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const GITHUB_RELEASE_API: &str =
    "https://api.github.com/repos/moxin-org/Moxin-Voice/releases/latest";
const UPDATE_CACHE_DIR_NAME: &str = "MoxinVoice/updates";
const INSTALL_SCRIPT_NAME: &str = "macos_install_update.sh";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedUpdate {
    pub version: String,
    pub dmg_path: PathBuf,
}

#[derive(Debug)]
pub enum CheckOutcome {
    NoUpdate,
    Ready {
        update: PreparedUpdate,
        fresh_download: bool,
    },
}

#[derive(Debug, Deserialize)]
struct GithubReleaseAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    assets: Vec<GithubReleaseAsset>,
}

pub fn display_version() -> String {
    std::env::var("MOXIN_APP_VERSION")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| crate::APP_VERSION.to_string())
}

pub fn current_app_bundle_path() -> Option<PathBuf> {
    if let Ok(resources) = std::env::var("MOXIN_APP_RESOURCES") {
        let resources = PathBuf::from(resources);
        if let Some(contents) = resources.parent() {
            if let Some(app) = contents.parent() {
                if app.extension() == Some(OsStr::new("app")) {
                    return Some(app.to_path_buf());
                }
            }
        }
    }

    let exe = std::env::current_exe().ok()?;
    for ancestor in exe.ancestors() {
        if ancestor.extension() == Some(OsStr::new("app")) {
            return Some(ancestor.to_path_buf());
        }
    }
    None
}

pub fn update_cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(UPDATE_CACHE_DIR_NAME)
}

pub fn check_and_prepare_update(current_version: &str) -> Result<CheckOutcome, String> {
    let latest_release = fetch_latest_release()?;
    let latest_version = normalize_version(&latest_release.tag_name);
    if !is_newer_version(&latest_version, current_version) {
        cleanup_cached_installers(None)?;
        return Ok(CheckOutcome::NoUpdate);
    }

    let asset = latest_release
        .assets
        .iter()
        .find(|asset| asset.name.to_ascii_lowercase().ends_with(".dmg"))
        .ok_or_else(|| "latest GitHub release has no DMG asset".to_string())?;

    let cache_dir = update_cache_dir();
    fs::create_dir_all(&cache_dir)
        .map_err(|err| format!("failed to create update cache dir: {}", err))?;

    let final_path = cache_dir.join(format!("Moxin-Voice-v{}.dmg", latest_version));
    cleanup_cached_installers(Some(final_path.as_path()))?;

    if final_path.exists() {
        return Ok(CheckOutcome::Ready {
            update: PreparedUpdate {
                version: latest_version,
                dmg_path: final_path,
            },
            fresh_download: false,
        });
    }

    let temp_path = cache_dir.join(format!("Moxin-Voice-v{}.download", latest_version));
    let _ = fs::remove_file(&temp_path);

    download_file(&asset.browser_download_url, &temp_path)?;
    fs::rename(&temp_path, &final_path)
        .map_err(|err| format!("failed to finalize downloaded update: {}", err))?;

    cleanup_cached_installers(Some(final_path.as_path()))?;

    Ok(CheckOutcome::Ready {
        update: PreparedUpdate {
            version: latest_version,
            dmg_path: final_path,
        },
        fresh_download: true,
    })
}

pub fn launch_update_installer(update: &PreparedUpdate) -> Result<(), String> {
    let script_path = resolve_install_script_path()
        .ok_or_else(|| "install helper script not found".to_string())?;

    let current_app =
        current_app_bundle_path().or_else(|| Some(PathBuf::from("/Applications/Moxin Voice.app")));
    let mut cmd = Command::new(&script_path);
    cmd.arg("--dmg")
        .arg(&update.dmg_path)
        .arg("--app-name")
        .arg("Moxin Voice")
        .arg("--wait-pid")
        .arg(std::process::id().to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    if let Some(current_app) = current_app {
        cmd.arg("--current-app").arg(current_app);
    }

    cmd.spawn()
        .map(|_| ())
        .map_err(|err| format!("failed to launch update installer: {}", err))
}

fn resolve_install_script_path() -> Option<PathBuf> {
    if let Ok(resources) = std::env::var("MOXIN_APP_RESOURCES") {
        let bundled = PathBuf::from(resources)
            .join("scripts")
            .join(INSTALL_SCRIPT_NAME);
        if bundled.exists() {
            return Some(bundled);
        }
    }

    let repo_script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts")
        .join(INSTALL_SCRIPT_NAME);
    if repo_script.exists() {
        return Some(repo_script);
    }

    None
}

fn fetch_latest_release() -> Result<GithubRelease, String> {
    let output = Command::new("curl")
        .args([
            "-fsSL",
            "-H",
            "Accept: application/vnd.github+json",
            "-H",
            "User-Agent: MoxinVoiceUpdater",
            GITHUB_RELEASE_API,
        ])
        .output()
        .map_err(|err| format!("failed to launch curl for update check: {}", err))?;

    if !output.status.success() {
        return Err(format!(
            "GitHub release check failed with status {}",
            output.status
        ));
    }

    serde_json::from_slice::<GithubRelease>(&output.stdout)
        .map_err(|err| format!("failed to parse GitHub release JSON: {}", err))
}

fn download_file(url: &str, output_path: &Path) -> Result<(), String> {
    let status = Command::new("curl")
        .args(["-fL", "--silent", "--show-error", "--output"])
        .arg(output_path)
        .arg(url)
        .status()
        .map_err(|err| format!("failed to launch curl for download: {}", err))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("update download failed with status {}", status))
    }
}

fn cleanup_cached_installers(keep_path: Option<&Path>) -> Result<(), String> {
    let cache_dir = update_cache_dir();
    if !cache_dir.exists() {
        return Ok(());
    }

    let keep_path = keep_path.and_then(|path| {
        path.canonicalize()
            .ok()
            .or_else(|| Some(path.to_path_buf()))
    });

    for entry in fs::read_dir(&cache_dir)
        .map_err(|err| format!("failed to read update cache dir: {}", err))?
    {
        let entry =
            entry.map_err(|err| format!("failed to inspect update cache entry: {}", err))?;
        let path = entry.path();
        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or_default();
        let is_update_artifact = matches!(extension, "dmg" | "download" | "tmp");
        if !is_update_artifact {
            continue;
        }

        let should_keep = keep_path
            .as_ref()
            .map(|keep| keep == &path)
            .unwrap_or(false);
        if should_keep {
            continue;
        }

        if path.is_file() {
            let _ = fs::remove_file(&path);
        }
    }

    Ok(())
}

fn normalize_version(version: &str) -> String {
    version.trim().trim_start_matches('v').to_string()
}

fn parse_version(version: &str) -> Vec<u32> {
    normalize_version(version)
        .split('.')
        .map(|segment| {
            let digits: String = segment
                .chars()
                .take_while(|ch| ch.is_ascii_digit())
                .collect();
            digits.parse::<u32>().unwrap_or(0)
        })
        .collect()
}

fn is_newer_version(candidate: &str, current: &str) -> bool {
    let candidate = parse_version(candidate);
    let current = parse_version(current);
    let max_len = candidate.len().max(current.len());

    for idx in 0..max_len {
        let lhs = *candidate.get(idx).unwrap_or(&0);
        let rhs = *current.get(idx).unwrap_or(&0);
        if lhs > rhs {
            return true;
        }
        if lhs < rhs {
            return false;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::{is_newer_version, normalize_version, parse_version};

    #[test]
    fn normalize_version_strips_v_prefix() {
        assert_eq!(normalize_version("v0.0.4"), "0.0.4");
        assert_eq!(normalize_version("0.0.5"), "0.0.5");
    }

    #[test]
    fn parse_version_handles_suffixes() {
        assert_eq!(parse_version("v1.2.3"), vec![1, 2, 3]);
        assert_eq!(parse_version("1.2.3-beta.1"), vec![1, 2, 3, 1]);
    }

    #[test]
    fn newer_version_comparison_is_numeric() {
        assert!(is_newer_version("0.0.10", "0.0.4"));
        assert!(is_newer_version("v0.1.0", "0.0.9"));
        assert!(!is_newer_version("0.0.4", "0.0.4"));
        assert!(!is_newer_version("0.0.3", "0.0.4"));
    }
}
