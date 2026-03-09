use std::fs;
use zed_extension_api::{
    self as zed, Command, ContextServerConfiguration, ContextServerId, GithubReleaseOptions,
    Project, Result,
};

const VERSION: &str = env!("CARGO_PKG_VERSION");

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
        // Fetch the latest release from GitHub
        let release = zed::latest_github_release(
            "arrrrny/zuraffa",
            GithubReleaseOptions {
                require_assets: true,
                pre_release: false,
            },
        )?;

        let (os, arch) = zed::current_platform();

        let (os_name, arch_name) = match (os, arch) {
            (zed::Os::Mac, zed::Architecture::Aarch64) => ("macos", "arm64"),
            (zed::Os::Mac, zed::Architecture::X8664) => ("macos", "x64"),
            (zed::Os::Linux, zed::Architecture::X8664) => ("linux", "x64"),
            (zed::Os::Windows, zed::Architecture::X8664) => ("windows", "x64"),
            _ => return Err(format!("Unsupported platform: {:?} {:?}", os, arch).into()),
        };

        let is_windows = os == zed::Os::Windows;
        let ext = if is_windows { ".exe" } else { "" };

        let server_filename = format!("zuraffa_mcp_server-{}-{}{}", os_name, arch_name, ext);
        let cli_filename = format!("zfa-{}-{}{}", os_name, arch_name, ext);

        // Both assets are raw binaries (uncompressed)
        let server_asset_name = server_filename.clone();
        let cli_asset_name = cli_filename.clone();

        // Use the release version as provided by GitHub (e.g., "v3.16.0")
        let version_dir = format!("mcp-server-zuraffa-{}", release.version);
        let server_path = format!("{}/{}", version_dir, server_filename);
        let cli_path = format!("{}/{}", version_dir, cli_filename);

        // Check if both binaries already exist
        if fs::metadata(&server_path).map_or(false, |m| m.is_file())
            && fs::metadata(&cli_path).map_or(false, |m| m.is_file())
        {
            self.cached_binary_path = Some(server_path.clone());
            return Ok(server_path);
        }

        // Create directory
        fs::create_dir_all(&version_dir).map_err(|e| e.to_string())?;

        // Find assets in the release
        let server_asset = release
            .assets
            .iter()
            .find(|a| a.name == server_asset_name)
            .ok_or_else(|| format!("Asset not found: {}", server_asset_name))?;
        let cli_asset = release
            .assets
            .iter()
            .find(|a| a.name == cli_asset_name)
            .ok_or_else(|| format!("Asset not found: {}", cli_asset_name))?;

        // Download server (uncompressed)
        zed::download_file(
            &server_asset.download_url,
            &server_path,
            zed::DownloadedFileType::Uncompressed,
        )?;
        zed::make_file_executable(&server_path)?;

        // Download CLI (raw binary)
        zed::download_file(
            &cli_asset.download_url,
            &cli_path,
            zed::DownloadedFileType::Uncompressed,
        )?;
        zed::make_file_executable(&cli_path)?;

        // Clean up old version directories
        if let Ok(entries) = fs::read_dir(".") {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy().to_string();
                if name_str.starts_with("mcp-server-zuraffa-") && name_str != version_dir {
                    fs::remove_dir_all(entry.path()).ok();
                }
            }
        }

        self.cached_binary_path = Some(server_path.clone());
        Ok(server_path)
    }
}

zed::register_extension!(ZuraffaExtension);
