//! Image loading settings and configuration

pub const DEFAULT_SUPPORTED_FORMATS: &[&str] = &["png", "jpg", "jpeg", "svg", "bmp", "gif"];

#[derive(Debug, Clone, PartialEq)]
pub enum FilenameTruncationStyle {
    /// No truncation - show full filename
    None,
    /// Start-end truncation with ellipsis (e.g., "verylongfi…name.txt")
    Ellipsis,
    /// Fade out at the end
    FadeEnd,
}

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
    // Filename display settings
    pub truncate_long_filenames: bool,
    pub max_filename_length: usize,
    pub truncation_style: FilenameTruncationStyle,
    pub ellipsis_char: String, // Customizable ellipsis character
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
            truncate_long_filenames: true, // Enabled by default
            max_filename_length: 25, // Default max length
            truncation_style: FilenameTruncationStyle::Ellipsis, // Default truncation style
            ellipsis_char: "…".to_string(), // Default ellipsis character
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

    /// Truncate a filename for display according to the current settings
    pub fn truncate_filename(&self, filename: &str) -> String {
        if !self.truncate_long_filenames || filename.len() <= self.max_filename_length {
            return filename.to_string();
        }

        match self.truncation_style {
            FilenameTruncationStyle::None => filename.to_string(),
            FilenameTruncationStyle::Ellipsis => {
                truncate_filename_with_ellipsis(filename, self.max_filename_length, &self.ellipsis_char)
            }
            FilenameTruncationStyle::FadeEnd => {
                // For now, FadeEnd behaves the same as ellipsis for text display
                // In a graphical implementation, this could render with a fade effect
                truncate_filename_with_ellipsis(filename, self.max_filename_length, &self.ellipsis_char)
            }
        }
    }

    /// Get the full filename for tooltip display
    pub fn get_full_filename_tooltip(&self, full_path: &std::path::Path) -> Option<String> {
        if let Some(filename) = full_path.file_name() {
            let filename_str = filename.to_string_lossy();
            if self.truncate_long_filenames && filename_str.len() > self.max_filename_length {
                Some(format!("Full filename: {}", filename_str))
            } else {
                None
            }
        } else {
            None
        }
    }
}

/// Truncate a filename using start-end ellipsis method
/// Preserves the file extension and shows both the beginning and end of the filename
fn truncate_filename_with_ellipsis(filename: &str, max_length: usize, ellipsis_char: &str) -> String {
    if filename.len() <= max_length {
        return filename.to_string();
    }

    // Find the extension (including the dot)
    let extension_start = filename.rfind('.').unwrap_or(filename.len());
    let name_part = &filename[..extension_start];
    let extension_part = &filename[extension_start..];

    // Reserve space for ellipsis (1 char) and extension
    let ellipsis = ellipsis_char;
    let available_for_name = max_length.saturating_sub(ellipsis.len() + extension_part.len());

    if available_for_name < 3 {
        // If we can't fit meaningful content, just show the start
        return format!(
            "{}{}",
            &filename[..max_length.saturating_sub(ellipsis.len())],
            ellipsis
        );
    }

    // Split available space between start and end, favoring the start slightly
    let start_chars = (available_for_name + 1) / 2;
    let end_chars = available_for_name - start_chars;

    if name_part.len() <= available_for_name {
        // If the name part fits, don't truncate
        filename.to_string()
    } else {
        let start_part = &name_part[..start_chars.min(name_part.len())];
        let end_part = if end_chars > 0 && end_chars < name_part.len() {
            &name_part[name_part.len() - end_chars..]
        } else {
            ""
        };

        format!("{}{}{}{}", start_part, ellipsis, end_part, extension_part)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filename_truncation_no_truncation_needed() {
        let settings = ImageLoadingSettings {
            truncate_long_filenames: true,
            max_filename_length: 50,
            truncation_style: FilenameTruncationStyle::Ellipsis,
            ..Default::default()
        };

        let short_filename = "short.jpg";
        assert_eq!(settings.truncate_filename(short_filename), short_filename);
    }

    #[test]
    fn test_filename_truncation_disabled() {
        let settings = ImageLoadingSettings {
            truncate_long_filenames: false,
            max_filename_length: 10,
            truncation_style: FilenameTruncationStyle::Ellipsis,
            ..Default::default()
        };

        let long_filename = "very_long_filename_that_should_not_be_truncated.jpg";
        assert_eq!(settings.truncate_filename(long_filename), long_filename);
    }

    #[test]
    fn test_filename_truncation_with_ellipsis() {
        let settings = ImageLoadingSettings {
            truncate_long_filenames: true,
            max_filename_length: 20,
            truncation_style: FilenameTruncationStyle::Ellipsis,
            ..Default::default()
        };

        let long_filename = "very_long_filename_example.jpg";
        let result = settings.truncate_filename(long_filename);

        // Should be truncated to approximately 20 characters
        assert!(result.len() <= 20);
        // Should contain ellipsis
        assert!(result.contains("…"));
        // Should preserve extension
        assert!(result.ends_with(".jpg"));
        // Should start with beginning of filename
        assert!(result.starts_with("very"));
    }

    #[test]
    fn test_filename_truncation_without_extension() {
        let settings = ImageLoadingSettings {
            truncate_long_filenames: true,
            max_filename_length: 15,
            truncation_style: FilenameTruncationStyle::Ellipsis,
            ..Default::default()
        };

        let long_filename = "very_long_filename_without_extension";
        let result = settings.truncate_filename(long_filename);

        assert!(result.len() <= 15);
        assert!(result.contains("…"));
    }

    #[test]
    fn test_truncate_filename_with_ellipsis_function() {
        // Test the internal function directly
        let result = truncate_filename_with_ellipsis("very_long_filename.txt", 15, "…");
        assert!(result.len() <= 15);
        assert!(result.contains("…"));
        assert!(result.ends_with(".txt"));

        // Test edge case with very short max length
        let result2 = truncate_filename_with_ellipsis("filename.txt", 8, "…");
        assert!(result2.len() <= 8);
        assert!(result2.contains("…"));
    }

    #[test]
    fn test_custom_ellipsis_character() {
        let mut settings = ImageLoadingSettings::default();
        settings.truncate_long_filenames = true;
        settings.max_filename_length = 20;
        settings.truncation_style = FilenameTruncationStyle::Ellipsis;
        settings.ellipsis_char = "...".to_string();

        let long_filename = "very_long_filename_example.jpg";
        let result = settings.truncate_filename(long_filename);

        assert!(result.len() <= 20);
        assert!(result.contains("..."));
        assert!(result.ends_with(".jpg"));
    }

    #[test]
    fn test_get_full_filename_tooltip() {
        let settings = ImageLoadingSettings {
            truncate_long_filenames: true,
            max_filename_length: 10,
            truncation_style: FilenameTruncationStyle::Ellipsis,
            ..Default::default()
        };

        let short_path = std::path::Path::new("short.jpg");
        assert!(settings.get_full_filename_tooltip(short_path).is_none());

        let long_path = std::path::Path::new("very_long_filename.jpg");
        let tooltip = settings.get_full_filename_tooltip(long_path);
        assert!(tooltip.is_some());
        assert!(tooltip.unwrap().contains("very_long_filename.jpg"));
    }
}
