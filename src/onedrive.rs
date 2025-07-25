//! OneDrive integration and file status detection

use std::path::PathBuf;

// OneDrive file status tracking
#[derive(Debug, Clone, PartialEq)]
pub enum OneDriveFileStatus {
    /// File is fully downloaded and available locally
    Local,
    /// File is stored online only (placeholder/stub file)
    OnlineOnly,
    /// File is partially downloaded
    PartiallyDownloaded,
    /// Not a OneDrive file
    NotOneDrive,
}

impl OneDriveFileStatus {
    pub fn icon(&self) -> &'static str {
        match self {
            OneDriveFileStatus::Local => "💾",
            OneDriveFileStatus::OnlineOnly => "☁️",
            OneDriveFileStatus::PartiallyDownloaded => "⬇️",
            OneDriveFileStatus::NotOneDrive => "📄",
        }
    }
    
    pub fn description(&self) -> &'static str {
        match self {
            OneDriveFileStatus::Local => "Local file (fully downloaded)",
            OneDriveFileStatus::OnlineOnly => "OneDrive online-only file",
            OneDriveFileStatus::PartiallyDownloaded => "OneDrive partially downloaded",
            OneDriveFileStatus::NotOneDrive => "Regular local file",
        }
    }
}

#[derive(Debug, Clone)]
pub struct FileInfo {
    pub path: PathBuf,
    pub onedrive_status: OneDriveFileStatus,
    pub estimated_download_size: Option<u64>, // Size in bytes if it needs to be downloaded
}

impl FileInfo {
    pub fn new(path: PathBuf) -> Self {
        let onedrive_status = get_onedrive_file_status(&path);
        let estimated_download_size = if matches!(onedrive_status, OneDriveFileStatus::OnlineOnly | OneDriveFileStatus::PartiallyDownloaded) {
            // Get the reported file size (which is the full file size for placeholders)
            std::fs::metadata(&path).ok().map(|m| m.len())
        } else {
            None
        };
        
        Self {
            path,
            onedrive_status,
            estimated_download_size,
        }
    }
    
    pub fn will_trigger_download(&self) -> bool {
        matches!(self.onedrive_status, OneDriveFileStatus::OnlineOnly | OneDriveFileStatus::PartiallyDownloaded)
    }
}

// Platform-specific OneDrive status detection
#[cfg(windows)]
pub fn get_onedrive_file_status(path: &std::path::Path) -> OneDriveFileStatus {
    use std::os::windows::fs::MetadataExt;
    
    // Check if path is in OneDrive folder
    let path_str = path.to_string_lossy().to_lowercase();
    if !(path_str.contains("onedrive") || path_str.contains("sharepoint")) {
        return OneDriveFileStatus::NotOneDrive;
    }
    
    // For files in OneDrive paths, check file attributes
    if let Ok(metadata) = std::fs::metadata(path) {
        let attributes = metadata.file_attributes();
        
        // Check for FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS (0x00400000)
        // This indicates an on-demand file that will trigger download when accessed
        const FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS: u32 = 0x00400000;
        const FILE_ATTRIBUTE_RECALL_ON_OPEN: u32 = 0x00040000;
        
        // Debug output for troubleshooting
        #[cfg(debug_assertions)]
        println!("OneDrive file check: {} - attributes: 0x{:08X}", path.display(), attributes);
        
        if (attributes & FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS) != 0 {
            #[cfg(debug_assertions)]
            println!("  -> OnlineOnly (RECALL_ON_DATA_ACCESS)");
            return OneDriveFileStatus::OnlineOnly;
        }
        
        if (attributes & FILE_ATTRIBUTE_RECALL_ON_OPEN) != 0 {
            #[cfg(debug_assertions)]
            println!("  -> PartiallyDownloaded (RECALL_ON_OPEN)");
            return OneDriveFileStatus::PartiallyDownloaded;
        }
        
        // If no recall attributes, it's fully local
        #[cfg(debug_assertions)]
        println!("  -> Local (no recall attributes)");
        return OneDriveFileStatus::Local;
    }
    
    // Default to assuming it's local if we can't determine status
    #[cfg(debug_assertions)]
    println!("OneDrive file check: {} - couldn't read metadata, assuming Local", path.display());
    OneDriveFileStatus::Local
}

#[cfg(not(windows))]
pub fn get_onedrive_file_status(_path: &std::path::Path) -> OneDriveFileStatus {
    // On non-Windows platforms, assume all files are local
    OneDriveFileStatus::NotOneDrive
}
