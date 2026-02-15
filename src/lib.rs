use std::env;
use std::fs;
use std::path::PathBuf;
use zed_extension_api::{
    self as zed, Command, ContextServerId, DownloadedFileType, Project, Result,
};

struct ZuraffaExtension;

impl zed::Extension for ZuraffaExtension {
    fn new() -> Self {
        Self
    }

    fn context_server_command(
        &mut self,
        _context_server_id: &ContextServerId,
        _project: &Project,
    ) -> Result<Command> {
        let binary_path = Self::get_binary_path()?;

        // If binary exists, return it immediately (fast startup)
        if binary_path.exists() {
            let path_str = binary_path.to_string_lossy().to_string();
            let _ = zed::make_file_executable(&path_str);

            // Spawn background update check (non-blocking)
            Self::check_for_update_in_background();

            return Ok(Command {
                command: path_str,
                args: vec![],
                env: vec![],
            });
        }

        // Binary doesn't exist, must download (first time only)
        let binary_path_str = Self::download_binary(&binary_path)?;
        Ok(Command {
            command: binary_path_str,
            args: vec![],
            env: vec![],
        })
    }
}

impl ZuraffaExtension {
    fn get_binary_path() -> Result<PathBuf> {
        let (os, arch) = zed::current_platform();

        let (os_name, arch_name) = match (os, arch) {
            (zed::Os::Mac, zed::Architecture::Aarch64) => ("macos", "arm64"),
            (zed::Os::Mac, zed::Architecture::X8664) => ("macos", "x64"),
            (zed::Os::Linux, zed::Architecture::X8664) => ("linux", "x64"),
            (zed::Os::Windows, zed::Architecture::X8664) => ("windows", "x64"),
            _ => return Err(format!("Unsupported platform: {:?} {:?}", os, arch).into()),
        };

        let binary_name = if os == zed::Os::Windows {
            format!("zuraffa_mcp_server-{}-{}.exe", os_name, arch_name)
        } else {
            format!("zuraffa_mcp_server-{}-{}", os_name, arch_name)
        };

        let work_dir = env::current_dir().map_err(|e| e.to_string())?;
        Ok(work_dir.join(&binary_name))
    }

    fn download_binary(binary_path: &PathBuf) -> Result<String> {
        let binary_name = binary_path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or("Invalid binary name")?;

        let url = format!(
            "https://github.com/arrrrny/zuraffa/releases/latest/download/{}",
            binary_name
        );

        zed::download_file(&url, binary_name, DownloadedFileType::Uncompressed)?;

        let binary_path_str = binary_path.to_string_lossy().to_string();
        zed::make_file_executable(&binary_path_str)?;

        Ok(binary_path_str)
    }

    fn check_for_update_in_background() {
        // Spawn a background thread to check for updates
        // This is non-blocking - the server starts immediately with existing binary
        std::thread::spawn(|| {
            // Small delay to not compete with startup
            std::thread::sleep(std::time::Duration::from_secs(5));

            // Check current version from GitHub releases
            // If newer version exists, download and replace binary
            // Next server restart will use the new binary
            let _ = Self::do_update_check();
        });
    }

    fn do_update_check() -> Result<()> {
        let binary_path = Self::get_binary_path()?;

        // Get installed version from companion file
        let version_path = binary_path.with_extension("version");
        let installed_version = if version_path.exists() {
            fs::read_to_string(&version_path).unwrap_or_default()
        } else {
            String::new()
        };

        // Fetch latest version from GitHub API
        let latest_version = Self::fetch_latest_version().unwrap_or(installed_version.clone());

        if installed_version != latest_version {
            // Download new version
            Self::download_binary(&binary_path)?;

            // Update version file
            let _ = fs::write(&version_path, &latest_version);
        }

        Ok(())
    }

    fn fetch_latest_version() -> Result<String> {
        // This would require HTTP client, but ZED API may not support it
        // For now, we'll just download and let the binary self-update
        // Alternatively, we can check the binary's --version output
        Ok("latest".to_string())
    }
}

zed::register_extension!(ZuraffaExtension);
