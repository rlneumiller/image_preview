//! Image loading settings and configuration

pub const DEFAULT_SUPPORTED_FORMATS: &[&str] = &["png", "jpg", "jpeg", "svg", "bmp", "gif"];

#[derive(Debug, Clone)]
pub struct ImageLoadingSettings {
    pub skip_large_images: bool,
    pub auto_scale_large_images: bool,
    pub auto_scale_to_fit: bool, // Scale images to fit within the display frame
    pub max_file_size_mb: Option<u32>, // None means no limit
    pub supported_formats: Vec<String>,
    pub svg_recolor_enabled: bool,
    pub svg_target_color: [u8; 3], // RGB values
    pub debug_onedrive_detection: bool, // Show debug info for OneDrive file detection
}

impl Default for ImageLoadingSettings {
    fn default() -> Self {
        Self {
            skip_large_images: false,
            auto_scale_large_images: true,
            auto_scale_to_fit: true, // Enabled by default
            max_file_size_mb: Some(100), // 100MB default limit
            supported_formats: DEFAULT_SUPPORTED_FORMATS
                .iter()
                .map(|s| s.to_string())
                .collect(),
            svg_recolor_enabled: false,
            svg_target_color: [128, 128, 128], // Default gray
            debug_onedrive_detection: false, // Disabled by default
        }
    }
}

impl ImageLoadingSettings {
    pub fn skip_large_images(mut self, skip: bool) -> Self {
        self.skip_large_images = skip;
        if skip {
            self.auto_scale_large_images = false;
        }
        self
    }

    pub fn auto_scale_large_images(mut self, auto_scale: bool) -> Self {
        self.auto_scale_large_images = auto_scale;
        if auto_scale {
            self.skip_large_images = false;
        }
        self
    }

    pub fn max_file_size_mb(mut self, size_mb: Option<u32>) -> Self {
        self.max_file_size_mb = size_mb;
        self
    }

    pub fn auto_scale_to_fit(mut self, auto_scale: bool) -> Self {
        self.auto_scale_to_fit = auto_scale;
        self
    }

    pub fn get_supported_extensions(&self) -> &[String] {
        &self.supported_formats
    }
}
