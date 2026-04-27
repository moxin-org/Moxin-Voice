use serde::Deserialize;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const GITHUB_RELEASE_API: &str =
    "https://api.github.com/repos/moxin-org/Moxin-Voice/releases/latest";
const UPDATE_CACHE_DIR_NAME: &str = "MoxinVoice/updates";
const INSTALL_SCRIPT_NAME: &str = "macos_install_update.sh";
const RELEASE_API_ENV: &str = "MOXIN_UPDATE_RELEASE_API";
const CACHE_DIR_ENV: &str = "MOXIN_UPDATE_CACHE_DIR";
const INSTALL_SCRIPT_ENV: &str = "MOXIN_UPDATE_INSTALL_SCRIPT";
const CURRENT_APP_ENV: &str = "MOXIN_UPDATE_CURRENT_APP";

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

#[derive(Clone, Debug)]
struct UpdateConfig {
    release_api: String,
    cache_dir: PathBuf,
    install_script_path: Option<PathBuf>,
    current_app_override: Option<PathBuf>,
}

impl UpdateConfig {
    fn from_env() -> Self {
        Self {
            release_api: read_env_path_or_string(RELEASE_API_ENV)
                .unwrap_or_else(|| GITHUB_RELEASE_API.to_string()),
            cache_dir: read_env_path(CACHE_DIR_ENV)
                .unwrap_or_else(default_update_cache_dir),
            install_script_path: read_env_path(INSTALL_SCRIPT_ENV),
            current_app_override: read_env_path(CURRENT_APP_ENV),
        }
    }
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
    UpdateConfig::from_env().cache_dir
}

pub fn check_and_prepare_update(current_version: &str) -> Result<CheckOutcome, String> {
    check_and_prepare_update_with_config(current_version, &UpdateConfig::from_env())
}

fn check_and_prepare_update_with_config(
    current_version: &str,
    config: &UpdateConfig,
) -> Result<CheckOutcome, String> {
    let latest_release = fetch_latest_release(config)?;
    let latest_version = normalize_version(&latest_release.tag_name);
    if !is_newer_version(&latest_version, current_version) {
        cleanup_cached_installers_in_dir(&config.cache_dir, None)?;
        return Ok(CheckOutcome::NoUpdate);
    }

    let asset = latest_release
        .assets
        .iter()
        .find(|asset| asset.name.to_ascii_lowercase().ends_with(".dmg"))
        .ok_or_else(|| "latest GitHub release has no DMG asset".to_string())?;

    let cache_dir = config.cache_dir.clone();
    fs::create_dir_all(&cache_dir)
        .map_err(|err| format!("failed to create update cache dir: {}", err))?;

    let final_path = cache_dir.join(format!("Moxin-Voice-v{}.dmg", latest_version));
    cleanup_cached_installers_in_dir(&cache_dir, Some(final_path.as_path()))?;

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

    cleanup_cached_installers_in_dir(&cache_dir, Some(final_path.as_path()))?;

    Ok(CheckOutcome::Ready {
        update: PreparedUpdate {
            version: latest_version,
            dmg_path: final_path,
        },
        fresh_download: true,
    })
}

pub fn launch_update_installer(update: &PreparedUpdate) -> Result<(), String> {
    launch_update_installer_with_config(update, &UpdateConfig::from_env())
}

fn launch_update_installer_with_config(
    update: &PreparedUpdate,
    config: &UpdateConfig,
) -> Result<(), String> {
    let script_path = resolve_install_script_path(config)
        .ok_or_else(|| "install helper script not found".to_string())?;

    let current_app = config
        .current_app_override
        .clone()
        .or_else(current_app_bundle_path)
        .or_else(|| Some(PathBuf::from("/Applications/Moxin Voice.app")));
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

fn resolve_install_script_path(config: &UpdateConfig) -> Option<PathBuf> {
    if let Some(path) = config.install_script_path.clone().filter(|path| path.exists()) {
        return Some(path);
    }

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

fn fetch_latest_release(config: &UpdateConfig) -> Result<GithubRelease, String> {
    let output = Command::new("curl")
        .args([
            "-fsSL",
            "-H",
            "Accept: application/vnd.github+json",
            "-H",
            "User-Agent: MoxinVoiceUpdater",
        ])
        .arg(&config.release_api)
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

fn cleanup_cached_installers_in_dir(cache_dir: &Path, keep_path: Option<&Path>) -> Result<(), String> {
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

        let current_path = path.canonicalize().unwrap_or_else(|_| path.clone());
        let should_keep = keep_path
            .as_ref()
            .map(|keep| keep == &current_path || keep == &path)
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

fn default_update_cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(UPDATE_CACHE_DIR_NAME)
}

fn read_env_path(name: &str) -> Option<PathBuf> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn read_env_path_or_string(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
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
    use super::{
        check_and_prepare_update_with_config, cleanup_cached_installers_in_dir,
        is_newer_version, launch_update_installer_with_config, normalize_version, parse_version,
        CheckOutcome, PreparedUpdate, UpdateConfig,
    };
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::thread;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    static TEST_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new(prefix: &str) -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let counter = TEST_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "{}-{}-{}-{}",
                prefix,
                std::process::id(),
                unique,
                counter
            ));
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn test_config(release_api: String, cache_dir: &Path) -> UpdateConfig {
        UpdateConfig {
            release_api,
            cache_dir: cache_dir.to_path_buf(),
            install_script_path: None,
            current_app_override: None,
        }
    }

    fn write_fake_release(dir: &Path, version: &str, dmg_path: &Path) -> PathBuf {
        let release_json = dir.join("latest.json");
        let dmg_url = format!("file://{}", dmg_path.display());
        let json = format!(
            "{{\"tag_name\":\"v{}\",\"assets\":[{{\"name\":\"Moxin-Voice-v{}.dmg\",\"browser_download_url\":\"{}\"}}]}}",
            version, version, dmg_url
        );
        fs::write(&release_json, json).unwrap();
        release_json
    }

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

    #[test]
    fn check_and_prepare_update_downloads_and_reuses_cached_installer() {
        let fixture_dir = TestDir::new("app-update-fixture");
        let cache_dir = TestDir::new("app-update-cache");
        let source_dmg = fixture_dir.path().join("Moxin-Voice-v0.0.5.dmg");
        fs::write(&source_dmg, b"fake dmg bytes").unwrap();
        let release_json = write_fake_release(fixture_dir.path(), "0.0.5", &source_dmg);
        let config = test_config(format!("file://{}", release_json.display()), cache_dir.path());

        let first = check_and_prepare_update_with_config("0.0.4", &config).unwrap();
        match first {
            CheckOutcome::Ready {
                update,
                fresh_download,
            } => {
                assert!(fresh_download);
                assert_eq!(update.version, "0.0.5");
                assert!(update.dmg_path.exists());
                assert_eq!(fs::read(&update.dmg_path).unwrap(), b"fake dmg bytes");
            }
            other => panic!("expected ready outcome, got {:?}", other),
        }

        let second = check_and_prepare_update_with_config("0.0.4", &config).unwrap();
        match second {
            CheckOutcome::Ready { fresh_download, .. } => assert!(!fresh_download),
            other => panic!("expected cached ready outcome, got {:?}", other),
        }
    }

    #[test]
    fn check_and_prepare_update_replaces_stale_cached_installer() {
        let fixture_dir = TestDir::new("app-update-fixture");
        let cache_dir = TestDir::new("app-update-cache");
        let stale = cache_dir.path().join("Moxin-Voice-v0.0.5.dmg");
        let partial = cache_dir.path().join("Moxin-Voice-v0.0.5.download");
        fs::write(&stale, b"stale").unwrap();
        fs::write(&partial, b"partial").unwrap();

        let source_dmg = fixture_dir.path().join("Moxin-Voice-v0.0.6.dmg");
        fs::write(&source_dmg, b"new").unwrap();
        let release_json = write_fake_release(fixture_dir.path(), "0.0.6", &source_dmg);
        let config = test_config(format!("file://{}", release_json.display()), cache_dir.path());

        let outcome = check_and_prepare_update_with_config("0.0.4", &config).unwrap();
        let new_path = match outcome {
            CheckOutcome::Ready { update, .. } => update.dmg_path,
            other => panic!("expected ready outcome, got {:?}", other),
        };

        assert_eq!(new_path.file_name().unwrap(), "Moxin-Voice-v0.0.6.dmg");
        assert!(new_path.exists());
        assert!(!stale.exists());
        assert!(!partial.exists());
    }

    #[test]
    fn cleanup_cached_installers_keeps_requested_path_only() {
        let cache_dir = TestDir::new("app-update-cache");
        let keep = cache_dir.path().join("keep.dmg");
        let remove_dmg = cache_dir.path().join("remove.dmg");
        let remove_tmp = cache_dir.path().join("remove.tmp");
        let ignore = cache_dir.path().join("notes.txt");
        fs::write(&keep, b"keep").unwrap();
        fs::write(&remove_dmg, b"remove").unwrap();
        fs::write(&remove_tmp, b"remove").unwrap();
        fs::write(&ignore, b"ignore").unwrap();

        cleanup_cached_installers_in_dir(cache_dir.path(), Some(&keep)).unwrap();

        assert!(keep.exists());
        assert!(!remove_dmg.exists());
        assert!(!remove_tmp.exists());
        assert!(ignore.exists());
    }

    #[test]
    fn launch_update_installer_uses_override_script_and_current_app() {
        let fixture_dir = TestDir::new("app-update-installer");
        let dmg_path = fixture_dir.path().join("Moxin-Voice-v0.0.5.dmg");
        let script_path = fixture_dir.path().join("fake-installer.sh");
        let args_log = fixture_dir.path().join("installer-args.txt");
        let current_app = fixture_dir.path().join("Moxin Voice.app");

        fs::write(&dmg_path, b"fake dmg bytes").unwrap();
        fs::write(
            &script_path,
            format!(
                "#!/usr/bin/env bash\nprintf '%s\n' \"$@\" > \"{}\"\n",
                args_log.display()
            ),
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&script_path).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&script_path, perms).unwrap();
        }

        let config = UpdateConfig {
            release_api: "unused".to_string(),
            cache_dir: fixture_dir.path().join("cache"),
            install_script_path: Some(script_path.clone()),
            current_app_override: Some(current_app.clone()),
        };
        let update = PreparedUpdate {
            version: "0.0.5".to_string(),
            dmg_path: dmg_path.clone(),
        };

        launch_update_installer_with_config(&update, &config).unwrap();

        for _ in 0..20 {
            if args_log.exists() {
                break;
            }
            thread::sleep(Duration::from_millis(50));
        }

        let args = fs::read_to_string(&args_log).unwrap();
        assert!(args.contains("--dmg"));
        assert!(args.contains(&dmg_path.display().to_string()));
        assert!(args.contains("--current-app"));
        assert!(args.contains(&current_app.display().to_string()));
    }
}
