use std::fs;
use std::time::{Duration, SystemTime};
use zed_extension_api::{
    self as zed, Command, ContextServerId, DownloadedFileType, Project, Result,
};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const MAX_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60); // 1 week

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
}

impl ZuraffaExtension {
    fn is_fresh(path: &str) -> bool {
        fs::metadata(path)
            .ok()
            .filter(|m| m.is_file())
            .and_then(|m| m.modified().ok())
            .and_then(|modified| SystemTime::now().duration_since(modified).ok())
            .is_some_and(|age| age < MAX_AGE)
    }

    fn get_or_download_binary(&mut self) -> Result<String> {
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

        let version_dir = format!("zuraffa-{}", VERSION);
        let server_path = format!("{}/{}", version_dir, server_filename);
        let cli_path = format!("{}/{}", version_dir, cli_filename);

        // 1. Return cached path if server binary is still fresh
        if let Some(path) = &self.cached_binary_path {
            if Self::is_fresh(path) {
                return Ok(path.clone());
            }
        }

        // 2. Check disk — both exist and fresh
        if Self::is_fresh(&server_path) && Self::is_fresh(&cli_path) {
            self.cached_binary_path = Some(server_path.clone());
            return Ok(server_path);
        }

        // 3. Download both binaries
        fs::create_dir_all(&version_dir).map_err(|e| e.to_string())?;

        let base_url = "https://github.com/arrrrny/zuraffa/releases/latest/download";

        // Download MCP server
        zed::download_file(
            &format!("{}/{}", base_url, server_filename),
            &server_path,
            DownloadedFileType::Uncompressed,
        )?;
        zed::make_file_executable(&server_path)?;

        // Download CLI binary
        zed::download_file(
            &format!("{}/{}", base_url, cli_filename),
            &cli_path,
            DownloadedFileType::Uncompressed,
        )?;
        zed::make_file_executable(&cli_path)?;

        // 4. Clean up old version directories
        if let Ok(entries) = fs::read_dir(".") {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str.starts_with("zuraffa-") && name_str != version_dir {
                    fs::remove_dir_all(entry.path()).ok();
                }
            }
        }

        self.cached_binary_path = Some(server_path.clone());
        Ok(server_path)
    }
}

zed::register_extension!(ZuraffaExtension);
