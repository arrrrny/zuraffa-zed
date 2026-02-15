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

        let binary_filename = if os == zed::Os::Windows {
            format!("zuraffa_mcp_server-{}-{}.exe", os_name, arch_name)
        } else {
            format!("zuraffa_mcp_server-{}-{}", os_name, arch_name)
        };

        let version_dir = format!("zuraffa-{}", VERSION);
        let binary_path = format!("{}/{}", version_dir, binary_filename);

        // 1. Return cached path if binary is still fresh
        if let Some(path) = &self.cached_binary_path {
            if Self::is_fresh(path) {
                return Ok(path.clone());
            }
        }

        // 2. Check disk — exists and less than a week old
        if Self::is_fresh(&binary_path) {
            self.cached_binary_path = Some(binary_path.clone());
            return Ok(binary_path);
        }

        // 3. Download (first install, new version, or stale)
        fs::create_dir_all(&version_dir).map_err(|e| e.to_string())?;

        let url = format!(
            "https://github.com/arrrrny/zuraffa/releases/latest/download/{}",
            binary_filename
        );
        zed::download_file(&url, &binary_path, DownloadedFileType::Uncompressed)?;
        zed::make_file_executable(&binary_path)?;

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

        self.cached_binary_path = Some(binary_path.clone());
        Ok(binary_path)
    }
}

zed::register_extension!(ZuraffaExtension);
