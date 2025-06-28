//! Image loading settings and configuration

#[derive(Debug, Clone)]
pub struct ImageLoadingSettings {
    pub max_texture_size: u32,
    pub skip_large_images: bool,
    pub auto_scale_large_images: bool,
    pub max_file_size_mb: Option<u32>, // None means no limit
    pub supported_formats: Vec<String>,
    pub svg_recolor_enabled: bool,
    pub svg_target_color: [u8; 3], // RGB values
}

impl Default for ImageLoadingSettings {
    fn default() -> Self {
        Self {
            max_texture_size: 16384,
            skip_large_images: false,
            auto_scale_large_images: true,
            max_file_size_mb: Some(100), // 100MB default limit
            supported_formats: vec![
                "png".to_string(),
                "jpg".to_string(),
                "jpeg".to_string(),
                "svg".to_string(),
                "bmp".to_string(),
                "gif".to_string(),
            ],
            svg_recolor_enabled: false,
            svg_target_color: [128, 128, 128], // Default gray
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

    pub fn max_texture_size(mut self, size: u32) -> Self {
        self.max_texture_size = size;
        self
    }
}
