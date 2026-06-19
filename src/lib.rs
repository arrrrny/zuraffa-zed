use std::fs;
use zed_extension_api::{
    self as zed, Command, ContextServerConfiguration, ContextServerId, GithubReleaseOptions,
    Project, Result,
};

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Fallback tag used when the GitHub API is unreachable (rate-limited, network issue, etc.).
/// The primary path ALWAYS tries `latest_github_release` first.
const FALLBACK_TAG: &str = "v5.2.2";

struct ZuraffaExtension {
    cached_binary_path: Option<String>,
}

impl zed::Extension for ZuraffaExtension {
    fn new() -> Self {
        Self {
            cached_binary_path: None,
        }
    }

    fn context_server_command(
        &mut self,
        _context_server_id: &ContextServerId,
        _project: &Project,
    ) -> Result<Command> {
        let binary_path = self.get_or_download_binary()?;

        Ok(Command {
            command: binary_path,
            args: vec![],
            env: vec![],
        })
    }

    fn context_server_configuration(
        &mut self,
        _context_server_id: &ContextServerId,
        _project: &Project,
    ) -> Result<Option<ContextServerConfiguration>> {
        let installation_instructions = format!(
            r#"# Zuraffa MCP Server

The Zuraffa MCP Server is installed, but it requires the Zuraffa Flutter package to work.

⚠️ Important Requirements

Flutter Project Only: Zuraffa only works inside Flutter projects (must have pubspec.yaml).
Add Dependency: Add this line to your pubspec.yaml under dependencies::
```
zuraffa: ^{}
```
Install: Run flutter pub get in your project.

Documentation: https://zuraffa.com/docs/features/mcp-server
Pub.dev: https://pub.dev/packages/zuraffa

Once the package is installed in a Flutter project, the MCP server will be ready to use.
"#,
            VERSION
        );

        Ok(Some(ContextServerConfiguration {
            installation_instructions,
            default_settings: "{}".to_string(),
            settings_schema: "{}".to_string(),
        }))
    }
}

impl ZuraffaExtension {
    fn get_or_download_binary(&mut self) -> Result<String> {
        if let Some(path) = &self.cached_binary_path {
            if fs::metadata(path).map_or(false, |m| m.is_file()) {
                return Ok(path.clone());
            }
        }

        let (os, arch) = zed::current_platform();
        let (os_name, arch_name) = Self::platform_names(os, arch)?;

        // Step 1: Always try the GitHub releases API first — gets the latest version
        match Self::try_api_download(os, &os_name, &arch_name) {
            Ok(server_path) => {
                self.cached_binary_path = Some(server_path.clone());
                return Ok(server_path);
            }
            Err(e) => {
                eprintln!("Zuraffa: API download failed: {}", e);
            }
        }

        // Step 2: Fallback — construct the download URL directly using the fallback tag.
        // This avoids the API entirely and works when GitHub is rate-limiting us.
        match Self::try_direct_download(os, &os_name, &arch_name, FALLBACK_TAG) {
            Ok(server_path) => {
                self.cached_binary_path = Some(server_path.clone());
                return Ok(server_path);
            }
            Err(e) => {
                eprintln!(
                    "Zuraffa: direct download failed for {}: {}",
                    FALLBACK_TAG, e
                );
            }
        }

        // Step 3: If both the API and direct download failed, report the error.
        Err("Failed to download zuraffa_mcp_server binary. Check your network connection and try again.".into())
    }

    fn platform_names(
        os: zed::Os,
        arch: zed::Architecture,
    ) -> Result<(&'static str, &'static str)> {
        let os_name = match os {
            zed::Os::Mac => "macos",
            zed::Os::Linux => "linux",
            zed::Os::Windows => "windows",
        };
        let arch_name = match arch {
            zed::Architecture::Aarch64 => "arm64",
            zed::Architecture::X8664 => "x64",
            _ => return Err(format!("Unsupported architecture: {:?}", arch).into()),
        };
        Ok((os_name, arch_name))
    }

    /// Check whether a release asset matches our target platform.
    fn asset_matches(asset: &zed::GithubReleaseAsset, os_name: &str, arch_name: &str) -> bool {
        let name = asset.name.to_lowercase();
        if !name.contains("zuraffa_mcp_server") {
            return false;
        }
        let os_ok = name.contains(os_name) || (os_name == "macos" && name.contains("darwin"));
        if !os_ok {
            return false;
        }
        name.contains(arch_name) || (arch_name == "arm64" && name.contains("aarch64"))
    }

    /// Returns true for compressed archive assets that should NOT be preferred
    /// over raw binaries.
    fn is_archive_asset(name: &str) -> bool {
        let lower = name.to_lowercase();
        lower.ends_with(".tar.gz")
            || lower.ends_with(".gz")
            || lower.ends_with(".zip")
            || lower.ends_with(".tar")
    }

    /// Preferred path: fetch the latest release via API, find the matching asset,
    /// and download it.
    fn try_api_download(os: zed::Os, os_name: &str, arch_name: &str) -> Result<String> {
        let release = zed::latest_github_release(
            "arrrrny/zuraffa",
            GithubReleaseOptions {
                require_assets: true,
                pre_release: false,
            },
        )?;

        let version = &release.version;
        let version_dir = format!("mcp-server-zuraffa-{}", version);
        let binary_ext = if os == zed::Os::Windows { ".exe" } else { "" };
        let server_path = format!("{}/zuraffa_mcp_server{}", version_dir, binary_ext);

        if fs::metadata(&server_path).map_or(false, |m| m.is_file()) {
            return Ok(server_path);
        }

        // Collect all assets matching our platform.
        let matches: Vec<_> = release
            .assets
            .iter()
            .filter(|a| Self::asset_matches(a, os_name, arch_name))
            .collect();

        // Prefer raw binaries over archives (.tar.gz / .gz / .zip).
        // Downloading a .tar.gz with Gzip only strips the gzip layer and
        // leaves a tar file — not an executable — which causes the server
        // to fail on startup.
        let asset = matches
            .iter()
            .find(|a| !Self::is_archive_asset(&a.name))
            .or_else(|| matches.first())
            .ok_or_else(|| format!("No zuraffa_mcp_server asset for {}/{}", os_name, arch_name))?;

        let download_type = if Self::is_archive_asset(&asset.name) {
            if asset.name.ends_with(".zip") {
                zed::DownloadedFileType::Zip
            } else {
                zed::DownloadedFileType::Gzip
            }
        } else {
            zed::DownloadedFileType::Uncompressed
        };

        fs::create_dir_all(&version_dir).map_err(|e| e.to_string())?;
        zed::download_file(&asset.download_url, &server_path, download_type)?;
        zed::make_file_executable(&server_path)?;
        Self::cleanup_old_versions(&version_dir);

        Ok(server_path)
    }

    /// Fallback: construct the GitHub release download URL from a known tag pattern.
    /// This does NOT call the GitHub API — it only needs the tag name.
    fn try_direct_download(
        os: zed::Os,
        os_name: &str,
        arch_name: &str,
        tag: &str,
    ) -> Result<String> {
        let version_dir = format!("mcp-server-zuraffa-{}", tag);
        let binary_ext = if os == zed::Os::Windows { ".exe" } else { "" };
        let server_path = format!("{}/zuraffa_mcp_server{}", version_dir, binary_ext);

        if fs::metadata(&server_path).map_or(false, |m| m.is_file()) {
            return Ok(server_path);
        }

        let asset_name = format!("zuraffa_mcp_server-{}-{}", os_name, arch_name);
        let download_url = format!(
            "https://github.com/arrrrny/zuraffa/releases/download/{}/{}",
            tag, asset_name
        );

        fs::create_dir_all(&version_dir).map_err(|e| e.to_string())?;
        zed::download_file(
            &download_url,
            &server_path,
            zed::DownloadedFileType::Uncompressed,
        )?;
        zed::make_file_executable(&server_path)?;
        Self::cleanup_old_versions(&version_dir);

        Ok(server_path)
    }

    fn cleanup_old_versions(current_version_dir: &str) {
        if let Ok(entries) = fs::read_dir(".") {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str.starts_with("mcp-server-zuraffa-") && name_str != current_version_dir {
                    fs::remove_dir_all(entry.path()).ok();
                }
            }
        }
    }
}

zed::register_extension!(ZuraffaExtension);
