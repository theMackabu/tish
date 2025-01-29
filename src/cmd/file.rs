use super::icons::{Icon, DIR_ICONS, EXT_ICONS, ICONS};
use std::{fs::Metadata, os::unix::fs::PermissionsExt, path::Path};

pub struct FileInfo {
    pub file_type: &'static str,
    pub icon: &'static Icon,
    pub display_name: String,
}

impl FileInfo {
    pub fn new(metadata: &Metadata, name: &str) -> Self {
        if metadata.is_dir() {
            Self::get_directory_info(name)
        } else if metadata.file_type().is_symlink() {
            Self::get_symlink_info(name)
        } else if metadata.permissions().mode() & 0o111 != 0 {
            Self::get_executable_info(name)
        } else {
            Self::get_file_info(name)
        }
    }

    fn get_directory_info(name: &str) -> Self {
        let name_lower = name.to_lowercase();
        let icon_key = DIR_ICONS.get(name_lower.as_str()).copied().unwrap_or_else(|| if name.starts_with('.') { "hiddendir" } else { "dir" });

        FileInfo {
            file_type: "directory",
            icon: ICONS.get(icon_key).unwrap_or_else(|| ICONS.get("dir").unwrap()),
            display_name: format!("{}/", name),
        }
    }

    fn get_symlink_info(name: &str) -> Self {
        FileInfo {
            file_type: "symlink",
            icon: ICONS.get("link").expect("Link icon should exist"),
            display_name: format!("{}@", name),
        }
    }

    fn get_executable_info(name: &str) -> Self {
        FileInfo {
            file_type: "executable",
            icon: ICONS.get("binary").expect("Binary icon should exist"),
            display_name: format!("{}*", name),
        }
    }

    fn get_file_info(name: &str) -> Self {
        let name_lower = name.to_lowercase();

        let icon_key = if let Some(&icon) = EXT_ICONS.get(name_lower.as_str()) {
            icon
        } else {
            let extension = Path::new(name).extension().and_then(|e| e.to_str()).map(str::to_lowercase).unwrap_or_default();
            EXT_ICONS.get(extension.as_str()).copied().unwrap_or_else(|| if name.starts_with('.') { "hiddenfile" } else { "file" })
        };

        FileInfo {
            file_type: "file",
            icon: ICONS.get(icon_key).unwrap_or_else(|| ICONS.get("file").unwrap()),
            display_name: name.to_string(),
        }
    }
}
