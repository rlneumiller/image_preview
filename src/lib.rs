//! Image Preview Application Library
//! 
//! A high-performance image viewer with OneDrive integration and performance benchmarking.

pub mod app;
pub mod benchmark;
pub mod settings;
pub mod image_processing;
pub mod onedrive;
pub mod file_locality;
pub mod icons;

// Re-export commonly used types
pub use app::ImageViewerApp;
pub use settings::ImageLoadingSettings;
pub use benchmark::{SystemPerformanceCategory, PerformanceProfile, BenchmarkResult};
pub use onedrive::{OneDriveFileStatus, FileInfo as OneDriveFileInfo};
pub use file_locality::{FileLocalityStatus, FileInfo};
