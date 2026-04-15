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
        if let Some(path) = &self.cached_binary_path {
            if fs::metadata(path).map_or(false, |m| m.is_file()) {
                return Ok(path.clone());
            }
        }

        let release = zed::latest_github_release(
            "arrrrny/zuraffa",
            GithubReleaseOptions {
                require_assets: true,
                pre_release: false,
            },
        )?;

        let (os, arch) = zed::current_platform();

        // Map to standardized naming conventions
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

        // Flexible asset finding (supports darwin/macos and arm64/x64 fallbacks)
        let find_asset = |name_prefix: &str| {
            release.assets.iter().find(|a| {
                let name = a.name.to_lowercase();
                name.contains(name_prefix) &&
                (name.contains(os_name) || (os_name == "macos" && name.contains("darwin"))) &&
                (name.contains(arch_name) || (arch_name == "arm64" && name.contains("x64")))
            })
        };

        let server_asset = find_asset("zuraffa_mcp_server")
            .ok_or_else(|| format!("Server asset not found for {}/{}", os_name, arch_name))?;

        let cli_asset = find_asset("zfa")
            .ok_or_else(|| format!("CLI asset not found for {}/{}", os_name, arch_name))?;

        let version_dir = format!("mcp-server-zuraffa-{}", release.version);
        let binary_ext = if os == zed::Os::Windows { ".exe" } else { "" };
        let server_path = format!("{}/zuraffa_mcp_server{}", version_dir, binary_ext);
        let cli_path = format!("{}/zfa{}", version_dir, binary_ext);

        if fs::metadata(&server_path).map_or(false, |m| m.is_file()) {
            self.cached_binary_path = Some(server_path.clone());
            return Ok(server_path);
        }

        fs::create_dir_all(&version_dir).map_err(|e| e.to_string())?;

        let get_download_type = |asset_name: &str| {
            if asset_name.ends_with(".gz") || asset_name.ends_with(".tar.gz") {
                zed::DownloadedFileType::Gzip
            } else if asset_name.ends_with(".zip") {
                zed::DownloadedFileType::Zip
            } else {
                zed::DownloadedFileType::Uncompressed
            }
        };

        // Download server
        zed::download_file(
            &server_asset.download_url,
            &server_path,
            get_download_type(&server_asset.name),
        )?;
        zed::make_file_executable(&server_path)?;

        // Download CLI
        zed::download_file(
            &cli_asset.download_url,
            &cli_path,
            get_download_type(&cli_asset.name),
        )?;
        zed::make_file_executable(&cli_path)?;

        // Cleanup old versions
        if let Ok(entries) = fs::read_dir(".") {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
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
