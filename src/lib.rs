//! Image Preview Application Library
//! 
//! A high-performance image viewer with OneDrive integration and performance benchmarking.

pub mod app;
pub mod benchmark;
pub mod settings;
pub mod image_processing;
pub mod onedrive;

// Re-export commonly used types
pub use app::ImageViewerApp;
pub use settings::ImageLoadingSettings;
pub use benchmark::{SystemPerformanceCategory, PerformanceProfile, BenchmarkResult};
pub use onedrive::{OneDriveFileStatus, FileInfo};

// Embedded reference JPG for performance benchmarking
pub const REFERENCE_JPG_BYTES: &[u8] = include_bytes!("../assets/313KB-2295X1034.jpg");
