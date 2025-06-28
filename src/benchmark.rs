//! Performance benchmarking functionality

use std::time::Instant;
use std::collections::HashMap;
use std::path::PathBuf;
use eframe::egui;
use egui::{ColorImage, TextureHandle};
use glob::glob;
use image::ImageReader;

use crate::onedrive::FileInfo;
use crate::REFERENCE_JPG_BYTES;

// Performance categories based on simple CPU benchmark
#[derive(Debug, Clone, PartialEq)]
pub enum SystemPerformanceCategory {
    LowPower,    // < 1000 score (old/low-power systems)
    Moderate,    // 1000-3000 score (typical laptops, older desktops)
    Good,        // 3000-6000 score (modern laptops, mid-range desktops)
    High,        // 6000-10000 score (high-end desktops, workstations)
    Excellent,   // > 10000 score (top-tier systems)
}

impl SystemPerformanceCategory {
    pub fn from_score(score: u32) -> Self {
        match score {
            0..=999 => SystemPerformanceCategory::LowPower,
            1000..=2999 => SystemPerformanceCategory::Moderate,
            3000..=5999 => SystemPerformanceCategory::Good,
            6000..=9999 => SystemPerformanceCategory::High,
            _ => SystemPerformanceCategory::Excellent,
        }
    }
    
    pub fn description(&self) -> &str {
        match self {
            SystemPerformanceCategory::LowPower => "Low Power",
            SystemPerformanceCategory::Moderate => "Moderate",
            SystemPerformanceCategory::Good => "Good",
            SystemPerformanceCategory::High => "High",
            SystemPerformanceCategory::Excellent => "Excellent",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ImageCharacteristics {
    pub file_size_mb: f64,
    pub width: u32,
    pub height: u32,
    pub megapixels: f64,
    pub format: String,
    pub bit_depth: Option<u8>,
}

impl ImageCharacteristics {
    pub fn new(path: &PathBuf, width: u32, height: u32, format: String) -> Self {
        let file_size_mb = std::fs::metadata(path)
            .map(|m| m.len() as f64 / (1024.0 * 1024.0))
            .unwrap_or(0.0);
        
        let megapixels = (width as f64 * height as f64) / 1_000_000.0;
        
        Self {
            file_size_mb,
            width,
            height,
            megapixels,
            format,
            bit_depth: None, // TODO: Extract from image metadata if needed
        }
    }
}

#[derive(Debug, Clone)]
pub struct BenchmarkResult {
    pub characteristics: ImageCharacteristics,
    pub decode_time_ms: f64,
    pub texture_creation_time_ms: f64,
    pub total_time_ms: f64,
    pub success: bool,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PerformanceProfile {
    pub benchmark_results: Vec<BenchmarkResult>,
    pub system_capabilities: SystemCapabilities,
    pub last_benchmark_time: Option<Instant>,
    pub reference_comparison: Option<PerformanceComparison>,
}

#[derive(Debug, Clone)]
pub struct SystemCapabilities {
    pub max_successful_megapixels: f64,
    pub avg_decode_time_per_mp: f64, // milliseconds per megapixel
    pub avg_texture_time_per_mp: f64,
    pub format_performance: HashMap<String, f64>, // format -> avg time per MP
}

#[derive(Debug, Clone)]
pub struct PerformanceComparison {
    pub performance_ratio: f64, // Current machine performance relative to build machine (1.0 = same, 0.5 = half speed, 2.0 = twice as fast)
    pub confidence_level: f64,  // 0.0 to 1.0, how confident we are in the estimate
}

#[derive(Debug, Clone)]
pub struct ReferenceBenchmark {
    pub image_characteristics: ImageCharacteristics,
    pub build_machine_result: BenchmarkResult,
    pub cpu_info: String,
    pub gpu_info: String,
}

impl Default for PerformanceProfile {
    fn default() -> Self {
        Self {
            benchmark_results: Vec::new(),
            system_capabilities: SystemCapabilities {
                max_successful_megapixels: 0.0,
                avg_decode_time_per_mp: 0.0,
                avg_texture_time_per_mp: 0.0,
                format_performance: HashMap::new(),
            },
            last_benchmark_time: None,
            reference_comparison: None,
        }
    }
}

impl PerformanceProfile {
    pub fn add_benchmark_result(&mut self, result: BenchmarkResult) {
        self.benchmark_results.push(result);
        self.update_system_capabilities();
    }
    
    pub fn update_system_capabilities(&mut self) {
        if self.benchmark_results.is_empty() {
            return;
        }
        
        let successful_results: Vec<_> = self.benchmark_results
            .iter()
            .filter(|r| r.success)
            .collect();
            
        if successful_results.is_empty() {
            return;
        }
        
        // Update max successful megapixels
        self.system_capabilities.max_successful_megapixels = successful_results
            .iter()
            .map(|r| r.characteristics.megapixels)
            .fold(0.0, f64::max);
        
        // Calculate average decode time per megapixel
        let total_decode_time: f64 = successful_results
            .iter()
            .map(|r| r.decode_time_ms)
            .sum();
        let total_megapixels: f64 = successful_results
            .iter()
            .map(|r| r.characteristics.megapixels)
            .sum();
        
        if total_megapixels > 0.0 {
            self.system_capabilities.avg_decode_time_per_mp = total_decode_time / total_megapixels;
        }
        
        // Calculate average texture creation time per megapixel
        let total_texture_time: f64 = successful_results
            .iter()
            .map(|r| r.texture_creation_time_ms)
            .sum();
        
        if total_megapixels > 0.0 {
            self.system_capabilities.avg_texture_time_per_mp = total_texture_time / total_megapixels;
        }
        
        // Update format-specific performance
        self.system_capabilities.format_performance.clear();
        let mut format_stats: HashMap<String, (f64, f64)> = HashMap::new(); // format -> (total_time, total_mp)
        
        for result in &successful_results {
            let entry = format_stats.entry(result.characteristics.format.clone())
                .or_insert((0.0, 0.0));
            entry.0 += result.total_time_ms;
            entry.1 += result.characteristics.megapixels;
        }
        
        for (format, (total_time, total_mp)) in format_stats {
            if total_mp > 0.0 {
                self.system_capabilities.format_performance.insert(format, total_time / total_mp);
            }
        }
    }
    
    pub fn estimate_render_time(&self, characteristics: &ImageCharacteristics) -> f64 {
        if self.benchmark_results.is_empty() {
            return 0.0; // No data available
        }
        
        // Get format-specific performance if available
        let time_per_mp = self.system_capabilities.format_performance
            .get(&characteristics.format)
            .copied()
            .unwrap_or(
                self.system_capabilities.avg_decode_time_per_mp + 
                self.system_capabilities.avg_texture_time_per_mp
            );
        
        time_per_mp * characteristics.megapixels
    }
    
    pub fn benchmark_reference_image(&mut self, ctx: &egui::Context) -> Option<BenchmarkResult> {
        // Benchmark the embedded reference JPG
        let start_time = Instant::now();
        
        // Try to decode the embedded image
        let decode_start = Instant::now();
        let decode_result = image::load_from_memory(REFERENCE_JPG_BYTES)
            .map_err(|e| format!("Failed to decode reference image: {}", e));
        let decode_time = decode_start.elapsed();
        
        match decode_result {
            Ok(img) => {
                let (width, height) = (img.width(), img.height());
                let reference_benchmark = get_reference_benchmark();
                let characteristics = ImageCharacteristics {
                    file_size_mb: reference_benchmark.image_characteristics.file_size_mb,
                    width,
                    height,
                    megapixels: (width * height) as f64 / 1_000_000.0,
                    format: "jpg".to_string(),
                    bit_depth: None,
                };
                
                // Try to create texture
                let texture_start = Instant::now();
                let texture_result = self.try_create_reference_texture(&img, ctx);
                let texture_time = texture_start.elapsed();
                
                let total_time = start_time.elapsed();
                
                let result = match texture_result {
                    Ok(_) => BenchmarkResult {
                        characteristics,
                        decode_time_ms: decode_time.as_secs_f64() * 1000.0,
                        texture_creation_time_ms: texture_time.as_secs_f64() * 1000.0,
                        total_time_ms: total_time.as_secs_f64() * 1000.0,
                        success: true,
                        error_message: None,
                    },
                    Err(e) => BenchmarkResult {
                        characteristics,
                        decode_time_ms: decode_time.as_secs_f64() * 1000.0,
                        texture_creation_time_ms: texture_time.as_secs_f64() * 1000.0,
                        total_time_ms: total_time.as_secs_f64() * 1000.0,
                        success: false,
                        error_message: Some(format!("Reference texture creation failed: {}", e)),
                    }
                };
                
                // Calculate performance comparison if successful
                if result.success {
                    let reference_benchmark = get_reference_benchmark();
                    let performance_ratio = reference_benchmark.build_machine_result.total_time_ms / result.total_time_ms;
                    self.reference_comparison = Some(PerformanceComparison {
                        performance_ratio,
                        confidence_level: 0.9, // High confidence since it's the same image
                    });
                }
                
                Some(result)
            }
            Err(e) => {
                eprintln!("Failed to benchmark reference image: {}", e);
                None
            }
        }
    }
    
    fn try_create_reference_texture(&self, img: &image::DynamicImage, ctx: &egui::Context) -> Result<TextureHandle, String> {
        let size = [img.width() as _, img.height() as _];
        let rgba = img.to_rgba8();
        let pixels = rgba.as_flat_samples();
        let color_image = ColorImage::from_rgba_unmultiplied(size, pixels.as_slice());
        
        Ok(ctx.load_texture(
            "reference_benchmark_image",
            color_image,
            Default::default(),
        ))
    }
}

// Simple benchmark that tests both CPU and storage performance for image viewing
// Focuses on the actual operations: file I/O, memory allocation, and basic arithmetic
pub fn run_simple_cpu_benchmark() -> u32 {
    let start_time = Instant::now();
    
    let mut score = 0u32;
    
    // Test 1: Storage I/O simulation (tests file system performance)
    let io_start = Instant::now();
    let test_file_path = "benchmark_test_file.tmp";
    
    // Write test - simulate saving processed image data
    let test_data = vec![0xAB; 500_000]; // 500KB test file (typical small image)
    let write_success = std::fs::write(test_file_path, &test_data).is_ok();
    
    // Read test - simulate loading image files
    let mut read_times = Vec::new();
    for _ in 0..5 {
        let read_start = Instant::now();
        if let Ok(data) = std::fs::read(test_file_path) {
            read_times.push(read_start.elapsed().as_millis());
            score += (data.len() / 10_000) as u32; // Factor in data size
        }
    }
    
    // Clean up test file
    let _ = std::fs::remove_file(test_file_path);
    
    let io_time = io_start.elapsed().as_millis();
    let avg_read_time = if !read_times.is_empty() {
        read_times.iter().sum::<u128>() / read_times.len() as u128
    } else {
        100 // Default penalty for failed I/O
    };
    
    // Storage performance factor (faster I/O = higher score)
    // Also factor in total I/O time
    let io_factor = if write_success && avg_read_time < 200 {
        2000.0 / ((avg_read_time + io_time).max(1) as f64) // Fast storage bonus
    } else {
        0.1 // Penalty for slow/failing storage
    };
    score += (io_factor * 1000.0) as u32;
    
    // Test 2: Memory allocation and copying (simulates image loading into RAM)
    for _ in 0..5 {
        let mut buffer = vec![0u8; 200_000]; // ~200KB buffer (typical small image)
        for i in 0..buffer.len() {
            buffer[i] = (i % 256) as u8;
        }
        // Simulate format conversion (like JPEG -> RGBA)
        let mut output = vec![0u32; buffer.len() / 4];
        for i in 0..output.len() {
            let base = i * 4;
            if base + 3 < buffer.len() {
                output[i] = ((buffer[base] as u32) << 24) |
                           ((buffer[base + 1] as u32) << 16) |
                           ((buffer[base + 2] as u32) << 8) |
                           (buffer[base + 3] as u32);
            }
        }
        score += (output.iter().map(|&x| x as u64).sum::<u64>() / 10_000_000) as u32;
    }
    
    // Test 3: Basic arithmetic (simulates scaling calculations)
    for i in 0..25_000 {
        let width = 1920;
        let height = 1080;
        let max_size = 1024;
        
        let scale_factor = if width > max_size || height > max_size {
            (max_size as f32 / width.max(height) as f32).min(1.0)
        } else {
            1.0
        };
        
        let new_width = (width as f32 * scale_factor) as u32;
        let new_height = (height as f32 * scale_factor) as u32;
        
        score += (new_width + new_height + i as u32) / 2000;
    }
    
    let elapsed = start_time.elapsed();
    
    // Normalize score based on execution time, but heavily weight I/O performance
    let time_factor = 50.0 / elapsed.as_millis().max(1) as f64;
    let final_score = (score as f64 * time_factor) as u32;
    
    // Clamp score to reasonable range
    final_score.min(15_000).max(50)
}

// Function to get reference benchmark data based on current system performance
pub fn get_reference_benchmark() -> ReferenceBenchmark {
    // Run a simple CPU benchmark to categorize system performance
    let cpu_score = run_simple_cpu_benchmark();
    let performance_category = SystemPerformanceCategory::from_score(cpu_score);
    
    // Base timing values for a "Good" performance system (our reference)
    let base_decode_time = 45.0; // ms
    let base_texture_time = 15.0; // ms
    
    // Adjust timings based on performance category
    let (decode_multiplier, texture_multiplier) = match performance_category {
        SystemPerformanceCategory::LowPower => (3.0, 2.5),    // Much slower
        SystemPerformanceCategory::Moderate => (1.8, 1.6),    // Slower
        SystemPerformanceCategory::Good => (1.0, 1.0),        // Reference baseline
        SystemPerformanceCategory::High => (0.7, 0.8),        // Faster
        SystemPerformanceCategory::Excellent => (0.4, 0.6),   // Much faster
    };
    
    let decode_time = base_decode_time * decode_multiplier;
    let texture_time = base_texture_time * texture_multiplier;
    let total_time = decode_time + texture_time;
    
    ReferenceBenchmark {
        image_characteristics: ImageCharacteristics {
            file_size_mb: 0.305, // ~313KB
            width: 2295,
            height: 1034,
            megapixels: 2.373, // 2.295 * 1.034
            format: "jpg".to_string(),
            bit_depth: None,
        },
        build_machine_result: BenchmarkResult {
            characteristics: ImageCharacteristics {
                file_size_mb: 0.305,
                width: 2295,
                height: 1034,
                megapixels: 2.373,
                format: "jpg".to_string(),
                bit_depth: None,
            },
            decode_time_ms: decode_time,
            texture_creation_time_ms: texture_time,
            total_time_ms: total_time,
            success: true,
            error_message: None,
        },
        cpu_info: format!("{} Performance (Score: {})", performance_category.description(), cpu_score),
        gpu_info: "Integrated Graphics".to_string(),
    }
}

pub fn find_asset_images() -> Vec<PathBuf> {
    let mut asset_paths = Vec::new();
    let extensions = ["png", "jpg", "jpeg", "bmp", "gif"];
    
    // Check assets folder
    for ext in extensions.iter() {
        if let Ok(paths) = glob(&format!("assets/*.{}", ext)) {
            for entry in paths {
                if let Ok(path) = entry {
                    // Only include files that won't trigger OneDrive downloads
                    let file_info = FileInfo::new(path.clone());
                    if !file_info.will_trigger_download() {
                        asset_paths.push(path);
                    }
                }
            }
        }
    }
    
    // If no assets folder images found, use current directory images
    if asset_paths.is_empty() {
        for ext in extensions.iter() {
            if let Ok(paths) = glob(&format!("*.{}", ext)) {
                for entry in paths {
                    if let Ok(path) = entry {
                        // Only include files that won't trigger OneDrive downloads
                        let file_info = FileInfo::new(path.clone());
                        if !file_info.will_trigger_download() {
                            asset_paths.push(path);
                        }
                    }
                }
            }
        }
    }
    
    asset_paths
}

pub fn benchmark_image(path: &PathBuf, ctx: &egui::Context) -> BenchmarkResult {
    // Skip OneDrive files during benchmarking to avoid triggering downloads
    let file_info = FileInfo::new(path.clone());
    if file_info.will_trigger_download() {
        let format = path.extension()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_lowercase();
        
        // Create a safe characteristics object using only metadata
        let file_size_mb = std::fs::metadata(path)
            .map(|m| m.len() as f64 / (1024.0 * 1024.0))
            .unwrap_or(0.0);
        
        return BenchmarkResult {
            characteristics: ImageCharacteristics {
                file_size_mb,
                width: 0, // Unknown - cannot determine without triggering download
                height: 0, // Unknown - cannot determine without triggering download
                megapixels: 0.0, // Unknown - cannot determine without triggering download
                format,
                bit_depth: None,
            },
            decode_time_ms: 0.0,
            texture_creation_time_ms: 0.0,
            total_time_ms: 0.0,
            success: false,
            error_message: Some("Skipped OneDrive file to avoid triggering download during benchmark".to_string()),
        };
    }
    
    let format = path.extension()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_lowercase();
        
    let start_time = Instant::now();
    
    // Try to decode the image
    let decode_start = Instant::now();
    let decode_result = ImageReader::open(path)
        .map_err(|e| format!("Failed to open image: {}", e))
        .and_then(|reader| reader.decode().map_err(|e| format!("Failed to decode image: {}", e)));
    let decode_time = decode_start.elapsed();
    
    match decode_result {
        Ok(img) => {
            let (width, height) = (img.width(), img.height());
            let characteristics = ImageCharacteristics::new(path, width, height, format);
            
            // Try to create texture
            let texture_start = Instant::now();
            let texture_result = try_create_texture(&img, ctx, path);
            let texture_time = texture_start.elapsed();
            
            let total_time = start_time.elapsed();
            
            match texture_result {
                Ok(_) => BenchmarkResult {
                    characteristics,
                    decode_time_ms: decode_time.as_secs_f64() * 1000.0,
                    texture_creation_time_ms: texture_time.as_secs_f64() * 1000.0,
                    total_time_ms: total_time.as_secs_f64() * 1000.0,
                    success: true,
                    error_message: None,
                },
                Err(e) => BenchmarkResult {
                    characteristics,
                    decode_time_ms: decode_time.as_secs_f64() * 1000.0,
                    texture_creation_time_ms: texture_time.as_secs_f64() * 1000.0,
                    total_time_ms: total_time.as_secs_f64() * 1000.0,
                    success: false,
                    error_message: Some(format!("Texture creation failed: {}", e)),
                }
            }
        }
        Err(e) => {
            let total_time = start_time.elapsed();
            
            // Create minimal characteristics for failed load
            let file_size_mb = std::fs::metadata(path)
                .map(|m| m.len() as f64 / (1024.0 * 1024.0))
                .unwrap_or(0.0);
            
            BenchmarkResult {
                characteristics: ImageCharacteristics {
                    file_size_mb,
                    width: 0,
                    height: 0,
                    megapixels: 0.0,
                    format,
                    bit_depth: None,
                },
                decode_time_ms: decode_time.as_secs_f64() * 1000.0,
                texture_creation_time_ms: 0.0,
                total_time_ms: total_time.as_secs_f64() * 1000.0,
                success: false,
                error_message: Some(e),
            }
        }
    }
}

fn try_create_texture(img: &image::DynamicImage, ctx: &egui::Context, path: &PathBuf) -> Result<TextureHandle, String> {
    let size = [img.width() as _, img.height() as _];
    let rgba = img.to_rgba8();
    let pixels = rgba.as_flat_samples();
    let color_image = ColorImage::from_rgba_unmultiplied(size, pixels.as_slice());
    
    let texture_name = format!("benchmark_{}", path.file_name().unwrap_or_default().to_string_lossy());
    
    Ok(ctx.load_texture(
        texture_name,
        color_image,
        Default::default(),
    ))
}
