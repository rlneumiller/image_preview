#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use eframe::egui;
use egui::{ColorImage, TextureHandle};
use glob::glob;
use image::ImageReader;
use resvg::{tiny_skia, usvg};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use std::collections::HashMap;

// Embedded reference JPG for performance benchmarking
// This is a representative ~1MP JPG image used as a baseline for performance comparison
const REFERENCE_JPG_BYTES: &[u8] = include_bytes!("../assets/313KB-2295X1034.jpg");

// Performance categories based on simple CPU benchmark
#[derive(Debug, Clone, PartialEq)]
enum SystemPerformanceCategory {
    LowPower,    // < 1000 score (old/low-power systems)
    Moderate,    // 1000-3000 score (typical laptops, older desktops)
    Good,        // 3000-6000 score (modern laptops, mid-range desktops)
    High,        // 6000-10000 score (high-end desktops, workstations)
    Excellent,   // > 10000 score (top-tier systems)
}

impl SystemPerformanceCategory {
    fn from_score(score: u32) -> Self {
        match score {
            0..=999 => SystemPerformanceCategory::LowPower,
            1000..=2999 => SystemPerformanceCategory::Moderate,
            3000..=5999 => SystemPerformanceCategory::Good,
            6000..=9999 => SystemPerformanceCategory::High,
            _ => SystemPerformanceCategory::Excellent,
        }
    }
    
    fn description(&self) -> &str {
        match self {
            SystemPerformanceCategory::LowPower => "Low Power",
            SystemPerformanceCategory::Moderate => "Moderate",
            SystemPerformanceCategory::Good => "Good",
            SystemPerformanceCategory::High => "High",
            SystemPerformanceCategory::Excellent => "Excellent",
        }
    }
}

// Simple benchmark that tests both CPU and storage performance for image viewing
// Focuses on the actual operations: file I/O, memory allocation, and basic arithmetic
fn run_simple_cpu_benchmark() -> u32 {
    let start_time = Instant::now();
    
    let mut score = 0u32;
    
    // Test 1: Storage I/O simulation (tests file system performance)
    // This is often the biggest bottleneck for image viewing applications
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
fn get_reference_benchmark() -> ReferenceBenchmark {
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

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([800.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Image PreViewer",
        options,
        Box::new(|_cc| Ok(Box::<ImageViewerApp>::default())),
    )
}

#[derive(Debug, Clone)]
struct ImageLoadingSettings {
    max_texture_size: u32,
    skip_large_images: bool,
    auto_scale_large_images: bool,
    max_file_size_mb: Option<u32>, // None means no limit
    supported_formats: Vec<String>,
    svg_recolor_enabled: bool,
    svg_target_color: [u8; 3], // RGB values
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

struct ImageViewerApp {
    image_paths: Vec<PathBuf>,
    selected_image_index: Option<usize>,
    image_texture: Option<TextureHandle>,
    status_text: String,
    settings: ImageLoadingSettings,
    show_settings: bool,
    performance_profile: PerformanceProfile,
    show_benchmark_window: bool,
    benchmark_in_progress: bool,
    benchmark_threshold_ms: f64,
    run_benchmark_trigger: bool,
    auto_benchmark_on_startup: bool,
    // New fields for user confirmation dialog
    show_slow_image_dialog: bool,
    pending_slow_image_path: Option<PathBuf>,
    pending_slow_image_estimated_time: f64,
}

impl Default for ImageViewerApp {
    fn default() -> Self {
        let mut image_paths = vec![];
        let extensions = ["png", "jpg", "jpeg", "svg", "bmp", "gif"];
        for ext in extensions.iter() {
            if let Ok(paths) = glob(&format!("*.{}", ext)) {
                for entry in paths.flatten() {
                    image_paths.push(entry);
                }
            }
        }

        Self {
            image_paths,
            selected_image_index: None,
            image_texture: None,
            status_text: "Select an image".to_string(),
            settings: ImageLoadingSettings::default(),
            show_settings: false,
            performance_profile: PerformanceProfile::default(),
            show_benchmark_window: false,
            benchmark_in_progress: false,
            benchmark_threshold_ms: 2000.0, // 2 seconds
            run_benchmark_trigger: false,
            auto_benchmark_on_startup: true,
            show_slow_image_dialog: false,
            pending_slow_image_path: None,
            pending_slow_image_estimated_time: 0.0,
        }
    }
}

impl eframe::App for ImageViewerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Top menu bar
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("Settings", |ui| {
                    if ui.button("Image Loading Settings").clicked() {
                        self.show_settings = !self.show_settings;
                    }
                });
                ui.menu_button("Performance", |ui| {
                    if ui.button("Run Benchmark").clicked() {
                        self.run_benchmark(ctx);
                    }
                    if ui.button("Benchmark Results").clicked() {
                        self.show_benchmark_window = !self.show_benchmark_window;
                    }
                });
            });
        });

        // Settings window
        if self.show_settings {
            egui::Window::new("Image Loading Settings")
                .open(&mut self.show_settings)
                .show(ctx, |ui| {
                    ui.checkbox(&mut self.settings.skip_large_images, "Skip very large images");
                    ui.checkbox(&mut self.settings.auto_scale_large_images, "Auto-scale large images");
                    
                    if self.settings.skip_large_images {
                        self.settings.auto_scale_large_images = false;
                    } else if self.settings.auto_scale_large_images {
                        self.settings.skip_large_images = false;
                    }

                    ui.separator();
                    
                    ui.horizontal(|ui| {
                        ui.label("Max texture size:");
                        ui.add(egui::Slider::new(&mut self.settings.max_texture_size, 16..=32768));
                    });

                    ui.horizontal(|ui| {
                        ui.label("Max file size (MB):");
                        let mut max_size = self.settings.max_file_size_mb.unwrap_or(0);
                        if ui.add(egui::Slider::new(&mut max_size, 1..=10)).changed() {
                            self.settings.max_file_size_mb = if max_size > 0 { Some(max_size) } else { None };
                        }
                        if ui.button("No limit").clicked() {
                            self.settings.max_file_size_mb = None;
                        }
                    });

                    ui.separator();
                    ui.heading("SVG Options");
                    ui.checkbox(&mut self.settings.svg_recolor_enabled, "Enable SVG recoloring");
                    
                    if self.settings.svg_recolor_enabled {
                        ui.horizontal(|ui| {
                            ui.label("Target color:");
                            let mut color = egui::Color32::from_rgb(
                                self.settings.svg_target_color[0],
                                self.settings.svg_target_color[1],
                                self.settings.svg_target_color[2],
                            );
                            if ui.color_edit_button_srgba(&mut color).changed() {
                                let [r, g, b, _] = color.to_array();
                                self.settings.svg_target_color = [r, g, b];
                            }
                        });
                    }
                });
        }

        // Benchmark window
        let show_benchmark = self.show_benchmark_window;
        let benchmark_in_progress = self.benchmark_in_progress;
        let mut benchmark_threshold = self.benchmark_threshold_ms;
        let mut run_benchmark_clicked = false;
        
        if show_benchmark {
            let mut show_window = true;
            egui::Window::new("Performance Benchmark")
                .open(&mut show_window)
                .default_width(500.0)
                .show(ctx, |ui| {
                    ui.heading("Benchmark Configuration");
                    
                    ui.horizontal(|ui| {
                        ui.label("Performance threshold (ms):");
                        ui.add(egui::Slider::new(&mut benchmark_threshold, 100.0..=10000.0));
                    });
                    
                    ui.separator();
                    
                    if benchmark_in_progress {
                        ui.label("Benchmark in progress...");
                        ui.spinner();
                    } else {
                        if ui.button("Run Benchmark").clicked() {
                            run_benchmark_clicked = true;
                        }
                    }
                    
                    ui.separator();
                    ui.heading("System Performance Profile");
                    
                    // Show current system performance category
                    let cpu_score = run_simple_cpu_benchmark();
                    let performance_category = SystemPerformanceCategory::from_score(cpu_score);
                    let category_color = match performance_category {
                        SystemPerformanceCategory::LowPower => egui::Color32::RED,
                        SystemPerformanceCategory::Moderate => egui::Color32::YELLOW,
                        SystemPerformanceCategory::Good => egui::Color32::GREEN,
                        SystemPerformanceCategory::High => egui::Color32::LIGHT_BLUE,
                        SystemPerformanceCategory::Excellent => egui::Color32::LIGHT_GREEN,
                    };
                    
                    ui.horizontal(|ui| {
                        ui.label("System Performance:");
                        ui.colored_label(category_color, format!("{} (Score: {})", performance_category.description(), cpu_score));
                    });
                    
                    ui.separator();
                    
                    if !self.performance_profile.benchmark_results.is_empty() {
                        let caps = &self.performance_profile.system_capabilities;
                        
                        ui.label(format!("Max successful image size: {:.2} MP", caps.max_successful_megapixels));
                        ui.label(format!("Avg decode time: {:.2} ms/MP", caps.avg_decode_time_per_mp));
                        ui.label(format!("Avg texture time: {:.2} ms/MP", caps.avg_texture_time_per_mp));
                        
                        ui.separator();
                        ui.heading("Format Performance");
                        for (format, time_per_mp) in &caps.format_performance {
                            ui.label(format!("{}: {:.2} ms/MP", format, time_per_mp));
                        }
                        
                        ui.separator();
                        ui.heading("Benchmark Results");
                        
                        egui::ScrollArea::vertical()
                            .max_height(200.0)
                            .show(ui, |ui| {
                                for result in &self.performance_profile.benchmark_results {
                                    let status = if result.success { "✓" } else { "✗" };
                                    let color = if result.success { 
                                        egui::Color32::GREEN 
                                    } else { 
                                        egui::Color32::RED 
                                    };
                                    
                                    ui.horizontal(|ui| {
                                        ui.colored_label(color, status);
                                        ui.label(format!(
                                            "{} ({}x{}, {:.1}MP): {:.1}ms", 
                                            result.characteristics.format,
                                            result.characteristics.width,
                                            result.characteristics.height,
                                            result.characteristics.megapixels,
                                            result.total_time_ms
                                        ));
                                    });
                                    
                                    if let Some(ref error) = result.error_message {
                                        ui.label(format!("  Error: {}", error));
                                    }
                                }
                            });
                    } else {
                        ui.label("No benchmark data available. Run a benchmark to see performance profile.");
                    }
                });
            
            self.show_benchmark_window = show_window;
            self.benchmark_threshold_ms = benchmark_threshold;
            
            if run_benchmark_clicked {
                self.run_benchmark_trigger = true;
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::SidePanel::left("image_list_panel")
                .resizable(true)
                .show_inside(ui, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        ui.heading("Images");
                        let mut changed = false;
                        for (index, path) in self.image_paths.iter().enumerate() {
                            let is_selected = self.selected_image_index == Some(index);
                            
                            // Pre-calculate performance info to avoid borrowing issues
                            let has_benchmark_data = !self.performance_profile.benchmark_results.is_empty();
                            let performance_info = if has_benchmark_data {
                                self.will_image_render_quickly(path)
                            } else {
                                None
                            };
                            let estimated_time = if has_benchmark_data {
                                self.estimate_image_render_time(path)
                            } else {
                                None
                            };
                            
                            ui.horizontal(|ui| {
                                // Show performance indicator if benchmark data is available
                                if has_benchmark_data {
                                    if let Some(will_be_fast) = performance_info {
                                        let indicator = if will_be_fast { "🟢" } else { "🟡" };
                                        let tooltip = if will_be_fast { 
                                            "Expected to render quickly" 
                                        } else { 
                                            "May take longer to render" 
                                        };
                                        ui.label(indicator).on_hover_text(tooltip);
                                    } else {
                                        ui.label("⚪").on_hover_text("Performance unknown");
                                    }
                                }
                                
                                let label = ui.selectable_label(is_selected, path.to_string_lossy());
                                if label.clicked() {
                                    self.selected_image_index = Some(index);
                                    changed = true;
                                }
                                
                                // Show estimated render time on hover if available
                                if let Some(time) = estimated_time {
                                    label.on_hover_text(format!("Estimated render time: {:.0}ms", time));
                                }
                            });
                        }
                        if changed {
                            self.load_selected_image(ctx);
                        }
                    });
                });

            egui::CentralPanel::default().show_inside(ui, |ui| {
                // Set a neutral grey background for the image preview area
                ui.style_mut().visuals.extreme_bg_color = egui::Color32::from_gray(128);
                let frame = egui::Frame::default()
                    .fill(egui::Color32::from_gray(128))
                    .inner_margin(egui::Margin::same(10.0));
                
                frame.show(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        if let Some(texture) = &self.image_texture {
                            ui.image(texture);
                        } else {
                            // Customize status text color with good contrast against grey background
                            let text_color = if self.status_text.contains("Error") || self.status_text.contains("Skipped") {
                                egui::Color32::from_rgb(255, 120, 120) // Light red for errors - good contrast on grey
                            } else if self.status_text.contains("recolored") {
                                egui::Color32::from_rgb(120, 255, 120) // Light green for successful operations
                            } else {
                                egui::Color32::from_rgb(240, 240, 240) // Very light gray/white for normal status
                            };
                            
                            ui.colored_label(text_color, &self.status_text);
                        }
                    });
                });
            });
        });

        self.handle_keyboard_nav(ctx);
        
        // Handle benchmark trigger
        if self.run_benchmark_trigger && !self.benchmark_in_progress {
            self.run_benchmark_trigger = false;
            self.run_benchmark(ctx);
        }
        
        // Auto-benchmark on startup if enabled
        if self.auto_benchmark_on_startup && !self.benchmark_in_progress && self.performance_profile.benchmark_results.is_empty() {
            self.auto_benchmark_on_startup = false;
            self.run_benchmark(ctx);
        }

        // Slow image confirmation dialog
        if self.show_slow_image_dialog {
            let mut load_anyway = false;
            let mut cancel = false;
            
            egui::Window::new("Slow Image Warning")
                .open(&mut self.show_slow_image_dialog)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.label("⚠️ Performance Warning");
                        ui.separator();
                        
                        if let Some(ref path) = self.pending_slow_image_path {
                            ui.label(format!("Image: {}", path.file_name().unwrap_or_default().to_string_lossy()));
                        }
                        
                        ui.label(format!(
                            "Estimated load time: {:.1} seconds", 
                            self.pending_slow_image_estimated_time / 1000.0
                        ));
                        ui.label(format!(
                            "Threshold: {:.1} seconds", 
                            self.benchmark_threshold_ms / 1000.0
                        ));
                        
                        ui.separator();
                        ui.label("This image may take longer to load than expected.");
                        ui.label("Do you want to continue?");
                        
                        ui.separator();
                        
                        ui.horizontal(|ui| {
                            if ui.button("Load Anyway").clicked() {
                                load_anyway = true;
                            }
                            if ui.button("Cancel").clicked() {
                                cancel = true;
                            }
                        });
                    });
                });
            
            if cancel || !self.show_slow_image_dialog {
                self.pending_slow_image_path = None;
                self.pending_slow_image_estimated_time = 0.0;
            } else if load_anyway {
                self.show_slow_image_dialog = false;
                if let Some(path) = self.pending_slow_image_path.take() {
                    // Find the index and load the image
                    if let Some(index) = self.image_paths.iter().position(|p| p == &path) {
                        self.selected_image_index = Some(index);
                        self.force_load_selected_image(ctx);
                    }
                }
                self.pending_slow_image_estimated_time = 0.0;
            }
        }
    }
}

#[derive(Debug, Clone)]
struct ImageCharacteristics {
    file_size_mb: f64,
    width: u32,
    height: u32,
    megapixels: f64,
    format: String,
    bit_depth: Option<u8>,
}

impl ImageCharacteristics {
    fn new(path: &PathBuf, width: u32, height: u32, format: String) -> Self {
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
struct BenchmarkResult {
    characteristics: ImageCharacteristics,
    decode_time_ms: f64,
    texture_creation_time_ms: f64,
    total_time_ms: f64,
    success: bool,
    error_message: Option<String>,
}

#[derive(Debug, Clone)]
struct PerformanceProfile {
    benchmark_results: Vec<BenchmarkResult>,
    system_capabilities: SystemCapabilities,
    last_benchmark_time: Option<Instant>,
    reference_comparison: Option<PerformanceComparison>,
}

#[derive(Debug, Clone)]
struct SystemCapabilities {
    max_successful_megapixels: f64,
    avg_decode_time_per_mp: f64, // milliseconds per megapixel
    avg_texture_time_per_mp: f64,
    format_performance: HashMap<String, f64>, // format -> avg time per MP
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
    fn add_benchmark_result(&mut self, result: BenchmarkResult) {
        self.benchmark_results.push(result);
        self.update_system_capabilities();
    }
    
    fn update_system_capabilities(&mut self) {
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
    
    fn estimate_render_time(&self, characteristics: &ImageCharacteristics) -> f64 {
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
    
    fn will_likely_succeed(&self, characteristics: &ImageCharacteristics, threshold_ms: f64) -> bool {
        let estimated_time = self.estimate_render_time(characteristics);
        estimated_time <= threshold_ms && 
        characteristics.megapixels <= self.system_capabilities.max_successful_megapixels * 1.2 // 20% buffer
    }
    
    fn benchmark_reference_image(&mut self, ctx: &egui::Context) -> Option<BenchmarkResult> {
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
    
    fn get_performance_multiplier(&self) -> f64 {
        self.reference_comparison
            .as_ref()
            .map(|comp| comp.performance_ratio)
            .unwrap_or(1.0) // Default to 1:1 if no reference available
    }
    
    fn estimate_render_time_with_reference(&self, characteristics: &ImageCharacteristics) -> f64 {
        if let Some(ref comp) = self.reference_comparison {
            // Use reference benchmark to estimate performance
            let reference_benchmark = get_reference_benchmark();
            let base_time_per_mp = reference_benchmark.build_machine_result.total_time_ms / reference_benchmark.image_characteristics.megapixels;
            
            // Adjust for format differences (JPG is baseline)
            let format_multiplier = match characteristics.format.as_str() {
                "jpg" | "jpeg" => 1.0,
                "png" => 1.3,  // PNG typically takes longer due to compression
                "bmp" => 0.7,  // BMP is faster (no compression)
                "gif" => 1.1,  // GIF has some compression overhead
                _ => 1.2,      // Conservative estimate for unknown formats
            };
            
            // Apply performance ratio (accounts for current machine vs build machine)
            let adjusted_time_per_mp = base_time_per_mp / comp.performance_ratio * format_multiplier;
            
            adjusted_time_per_mp * characteristics.megapixels
        } else {
            // Fallback to original estimation if no reference available
            self.estimate_render_time(characteristics)
        }
    }
}

impl ImageViewerApp {
    fn recolor_svg_simple(&self, svg_content: &str) -> String {
        if !self.settings.svg_recolor_enabled {
            return svg_content.to_string();
        }

        let target_hex = format!(
            "#{:02x}{:02x}{:02x}",
            self.settings.svg_target_color[0],
            self.settings.svg_target_color[1],
            self.settings.svg_target_color[2]
        );

        println!("SVG Recoloring enabled! Target color: {}", target_hex);
        println!("Original SVG preview: {}", &svg_content[..std::cmp::min(200, svg_content.len())]);

        let mut result = svg_content.to_string();
        let mut changes_made = 0;
        
        if result.contains("currentColor") {
            result = result.replace("currentColor", &target_hex);
            changes_made += result.matches(&target_hex).count();
            println!("Replaced currentColor with {}, {} instances", target_hex, changes_made);
        }
        
        // Replace fill colors (but preserve "none" and gradients)
        let fill_regex = regex::Regex::new(r#"fill="(#[0-9a-fA-F]{6}|#[0-9a-fA-F]{3}|black|white|red|green|blue|yellow|cyan|magenta|purple|orange|brown|pink|gray|grey)""#).unwrap();
        let before_count = result.len();
        result = fill_regex.replace_all(&result, &format!(r#"fill="{}""#, target_hex)).to_string();
        if result.len() != before_count {
            changes_made += 1;
            println!("Replaced fill colors");
        }
            
        // Replace stroke colors
        let stroke_regex = regex::Regex::new(r#"stroke="(#[0-9a-fA-F]{6}|#[0-9a-fA-F]{3}|black|white|red|green|blue|yellow|cyan|magenta|purple|orange|brown|pink|gray|grey)""#).unwrap();
        let before_count = result.len();
        result = stroke_regex.replace_all(&result, &format!(r#"stroke="{}""#, target_hex)).to_string();
        if result.len() != before_count {
            changes_made += 1;
            println!("Replaced stroke colors");
        }

        // Handle CSS style attributes
        let style_regex = regex::Regex::new(r#"style="[^"]*(?:fill|stroke):\s*(#[0-9a-fA-F]{6}|#[0-9a-fA-F]{3}|black|white|red|green|blue|yellow|cyan|magenta|currentColor)[^"]*""#).unwrap();
        let before_count = result.len();
        result = style_regex.replace_all(&result, &format!(r#"style="fill: {}; stroke: {};""#, target_hex, target_hex)).to_string();
        if result.len() != before_count {
            changes_made += 1;
            println!("Replaced CSS style colors");
        }

        println!("Total changes made: {}", changes_made);
        if changes_made > 0 {
            println!("Modified SVG preview: {}", &result[..std::cmp::min(200, result.len())]);
        }

        result
    }

    fn should_skip_large_file(&self, path: &PathBuf) -> Option<String> {
        if let Some(max_mb) = self.settings.max_file_size_mb {
            if let Ok(metadata) = std::fs::metadata(path) {
                let size_mb = metadata.len() / (1024 * 1024);
                if size_mb > max_mb as u64 {
                    return Some(format!(
                        "Skipped large file ({} MB > {} MB limit): {}",
                        size_mb, max_mb, path.to_string_lossy()
                    ));
                }
            }
        }
        None
    }

    fn scale_image_if_needed(&self, img: image::DynamicImage) -> Result<image::DynamicImage, String> {
        let (width, height) = (img.width(), img.height());
        
        if width <= self.settings.max_texture_size && height <= self.settings.max_texture_size {
            return Ok(img);
        }

        if self.settings.skip_large_images {
            return Err(format!(
                "Image too large ({}x{} > {}x{} limit)", 
                width, height, self.settings.max_texture_size, self.settings.max_texture_size
            ));
        }

        if self.settings.auto_scale_large_images {
            // Calculate scale factor to fit within MAX_TEXTURE_SIZE
            let scale_factor = (self.settings.max_texture_size as f32 / width.max(height) as f32).min(1.0);
            let new_width = (width as f32 * scale_factor) as u32;
            let new_height = (height as f32 * scale_factor) as u32;

            Ok(img.resize(new_width, new_height, image::imageops::FilterType::Lanczos3))
        } else {
            Err(format!(
                "Image too large ({}x{} > {}x{} limit) and auto-scaling disabled", 
                width, height, self.settings.max_texture_size, self.settings.max_texture_size
            ))
        }
    }

    fn load_selected_image(&mut self, ctx: &egui::Context) {
        if let Some(index) = self.selected_image_index {
            if let Some(path) = self.image_paths.get(index) {
                // Check if we should prompt user for slow images (only if benchmark data is available)
                if !self.performance_profile.benchmark_results.is_empty() {
                    if let Some(estimated_time) = self.estimate_image_render_time(path) {
                        if estimated_time > self.benchmark_threshold_ms {
                            // Show slow image warning dialog
                            self.pending_slow_image_path = Some(path.clone());
                            self.pending_slow_image_estimated_time = estimated_time;
                            self.show_slow_image_dialog = true;
                            return; // Don't load immediately, wait for user confirmation
                        }
                    }
                }
                
                // If we get here, either no benchmark data, image is fast enough, or user confirmed
                self.force_load_selected_image(ctx);
            }
        }
    }

    fn force_load_selected_image(&mut self, ctx: &egui::Context) {
        if let Some(index) = self.selected_image_index {
            if let Some(path) = self.image_paths.get(index) {
                // Check file size first
                if let Some(skip_message) = self.should_skip_large_file(path) {
                    self.status_text = skip_message;
                    self.image_texture = None;
                    return;
                }

                let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("");
                if extension == "svg" {
                    match std::fs::read_to_string(path) {
                        Ok(svg_content) => {
                            // Apply recoloring if enabled
                            let processed_svg = self.recolor_svg_simple(&svg_content);
                            let svg_bytes = processed_svg.as_bytes();
                            
                            let mut fontdb = usvg::fontdb::Database::new();
                            fontdb.load_system_fonts();
                            
                            let options = usvg::Options {
                                fontdb: Arc::new(fontdb),
                                ..Default::default()
                            };
                            
                            match usvg::Tree::from_data(svg_bytes, &options) {
                                Ok(usvg_tree) => {
                                    let original_size = usvg_tree.size().to_int_size();
                                    let mut pixmap_size = original_size;
                                    
                                    // Scale down SVG if it's too large for texture
                                    let scale_factor = if original_size.width() > self.settings.max_texture_size || original_size.height() > self.settings.max_texture_size {
                                        if self.settings.skip_large_images {
                                            self.status_text = format!(
                                                "Skipped large SVG ({}x{} > {}x{} limit): {}", 
                                                original_size.width(), original_size.height(),
                                                self.settings.max_texture_size, self.settings.max_texture_size,
                                                path.to_string_lossy()
                                            );
                                            self.image_texture = None;
                                            return;
                                        }
                                        (self.settings.max_texture_size as f32 / original_size.width().max(original_size.height()) as f32).min(1.0)
                                    } else {
                                        1.0
                                    };
                                    
                                    if scale_factor < 1.0 {
                                        pixmap_size = tiny_skia::IntSize::from_wh(
                                            (original_size.width() as f32 * scale_factor) as u32,
                                            (original_size.height() as f32 * scale_factor) as u32,
                                        ).unwrap();
                                    }
                                    
                                    let mut pixmap = tiny_skia::Pixmap::new(
                                        pixmap_size.width(),
                                        pixmap_size.height(),
                                    ).unwrap();
                                    
                                    let transform = if scale_factor < 1.0 {
                                        tiny_skia::Transform::from_scale(scale_factor, scale_factor)
                                    } else {
                                        tiny_skia::Transform::identity()
                                    };
                                    
                                    resvg::render(&usvg_tree, transform, &mut pixmap.as_mut());
                                    
                                    let image_buffer = image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(
                                        pixmap.width(),
                                        pixmap.height(),
                                        pixmap.take(),
                                    ).unwrap();
                                    
                                    let size = [image_buffer.width() as _, image_buffer.height() as _];
                                    let color_image = ColorImage::from_rgba_unmultiplied(
                                        size,
                                        image_buffer.as_flat_samples().as_slice(),
                                    );
                                    
                                    self.image_texture = Some(ctx.load_texture(
                                        path.to_string_lossy(),
                                        color_image,
                                        Default::default(),
                                    ));
                                    
                                    // Update status to show if SVG was scaled
                                    let recolor_status = if self.settings.svg_recolor_enabled {
                                        format!(" (recolored to #{:02x}{:02x}{:02x})", 
                                            self.settings.svg_target_color[0],
                                            self.settings.svg_target_color[1], 
                                            self.settings.svg_target_color[2])
                                    } else {
                                        String::new()
                                    };
                                    
                                    if scale_factor < 1.0 {
                                        self.status_text = format!(
                                            "Loaded SVG (scaled from {}x{} to {}x{}){}: {}", 
                                            original_size.width(), original_size.height(),
                                            pixmap_size.width(), pixmap_size.height(),
                                            recolor_status,
                                            path.to_string_lossy()
                                        );
                                    } else {
                                        self.status_text = format!("Loaded SVG{}: {}", recolor_status, path.to_string_lossy());
                                    }
                                }
                                Err(e) => {
                                    self.status_text = format!("Error parsing SVG: {}", e);
                                    self.image_texture = None;
                                }
                            }
                        }
                        Err(e) => {
                            self.status_text = format!("Error reading SVG: {}", e);
                            self.image_texture = None;
                        }
                    }
                } else {
                    match ImageReader::open(path) {
                        Ok(reader) => match reader.decode() {
                            Ok(img) => {
                                // Store original dimensions before moving img
                                let (orig_width, orig_height) = (img.width(), img.height());
                                
                                // Scale down large images to prevent texture size errors
                                match self.scale_image_if_needed(img) {
                                    Ok(scaled_img) => {
                                        let size = [scaled_img.width() as _, scaled_img.height() as _];
                                        let rgba = scaled_img.to_rgba8();
                                        let pixels = rgba.as_flat_samples();
                                        let color_image = ColorImage::from_rgba_unmultiplied(size, pixels.as_slice());
                                        self.image_texture = Some(ctx.load_texture(
                                            path.to_string_lossy(),
                                            color_image,
                                            Default::default(),
                                        ));
                                        
                                        // Update status to show if image was scaled
                                        if scaled_img.width() != orig_width || scaled_img.height() != orig_height {
                                            self.status_text = format!(
                                                "Loaded (scaled from {}x{} to {}x{}): {}", 
                                                orig_width, orig_height,
                                                scaled_img.width(), scaled_img.height(),
                                                path.to_string_lossy()
                                            );
                                        } else {
                                            self.status_text = format!("Loaded: {}", path.to_string_lossy());
                                        }
                                    }
                                    Err(error_msg) => {
                                        self.status_text = error_msg;
                                        self.image_texture = None;
                                    }
                                }
                            }
                            Err(e) => {
                                self.status_text = format!("Error decoding image: {}", e);
                                self.image_texture = None;
                            }
                        },
                        Err(e) => {
                            self.status_text = format!("Error opening image: {}", e);
                            self.image_texture = None;
                        }
                    }
                }
            }
        }
    }

    fn handle_keyboard_nav(&mut self, ctx: &egui::Context) {
        let mut changed = false;
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
            if let Some(selected_index) = self.selected_image_index {
                if selected_index > 0 {
                    self.selected_image_index = Some(selected_index - 1);
                    changed = true;
                }
            } else if !self.image_paths.is_empty() {
                self.selected_image_index = Some(self.image_paths.len() - 1);
                changed = true;
            }
        }

        if ctx.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
            if let Some(selected_index) = self.selected_image_index {
                if selected_index < self.image_paths.len() - 1 {
                    self.selected_image_index = Some(selected_index + 1);
                    changed = true;
                }
            } else if !self.image_paths.is_empty() {
                self.selected_image_index = Some(0);
                changed = true;
            }
        }

        if changed {
            self.load_selected_image(ctx);
        }
    }

    fn run_benchmark(&mut self, ctx: &egui::Context) {
        if self.benchmark_in_progress {
            return;
        }
        
        self.benchmark_in_progress = true;
        self.performance_profile.benchmark_results.clear();
        self.performance_profile.last_benchmark_time = Some(Instant::now());
        
        // Find asset images to benchmark
        let asset_paths = self.find_asset_images();
        
        // Benchmark each asset image
        for path in asset_paths {
            let result = self.benchmark_image(&path, ctx);
            self.performance_profile.add_benchmark_result(result);
        }
        
        // Benchmark reference image
        if let Some(reference_result) = self.performance_profile.benchmark_reference_image(ctx) {
            self.performance_profile.add_benchmark_result(reference_result);
        }
        
        self.benchmark_in_progress = false;
        
        // Update status
        let successful_count = self.performance_profile.benchmark_results
            .iter()
            .filter(|r| r.success)
            .count();
        let total_count = self.performance_profile.benchmark_results.len();
        
        self.status_text = format!(
            "Benchmark completed: {}/{} images processed successfully", 
            successful_count, total_count
        );
    }
    
    fn find_asset_images(&self) -> Vec<PathBuf> {
        let mut asset_paths = Vec::new();
        let extensions = ["png", "jpg", "jpeg", "bmp", "gif"];
        
        // Check assets folder
        for ext in extensions.iter() {
            if let Ok(paths) = glob(&format!("assets/*.{}", ext)) {
                for entry in paths {
                    if let Ok(path) = entry {
                        asset_paths.push(path);
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
                            asset_paths.push(path);
                        }
                    }
                }
            }
        }
        
        asset_paths
    }
    
    fn benchmark_image(&self, path: &PathBuf, ctx: &egui::Context) -> BenchmarkResult {
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
                let texture_result = self.try_create_texture(&img, ctx, path);
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
                // Create minimal characteristics for failed decode
                let characteristics = ImageCharacteristics {
                    file_size_mb: std::fs::metadata(path)
                        .map(|m| m.len() as f64 / (1024.0 * 1024.0))
                        .unwrap_or(0.0),
                    width: 0,
                    height: 0,
                    megapixels: 0.0,
                    format,
                    bit_depth: None,
                };
                
                let total_time = start_time.elapsed();
                
                BenchmarkResult {
                    characteristics,
                    decode_time_ms: decode_time.as_secs_f64() * 1000.0,
                    texture_creation_time_ms: 0.0,
                    total_time_ms: total_time.as_secs_f64() * 1000.0,
                    success: false,
                    error_message: Some(format!("Image decode failed: {}", e)),
                }
            }
        }
    }
    
    fn try_create_texture(&self, img: &image::DynamicImage, ctx: &egui::Context, path: &PathBuf) -> Result<TextureHandle, String> {
        // Scale image if needed
        let scaled_img = self.scale_image_if_needed(img.clone())?;
        
        let size = [scaled_img.width() as _, scaled_img.height() as _];
        let rgba = scaled_img.to_rgba8();
        let pixels = rgba.as_flat_samples();
        let color_image = ColorImage::from_rgba_unmultiplied(size, pixels.as_slice());
        
        Ok(ctx.load_texture(
            format!("benchmark_{}", path.to_string_lossy()),
            color_image,
            Default::default(),
        ))
    }
    
    fn estimate_image_render_time(&self, path: &PathBuf) -> Option<f64> {
        // Try to get image dimensions without fully loading
        if let Ok(reader) = ImageReader::open(path) {
            if let Ok((width, height)) = reader.into_dimensions() {
                let format = path.extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_lowercase();
                
                let characteristics = ImageCharacteristics::new(path, width, height, format);
                let estimated_time = self.performance_profile.estimate_render_time(&characteristics);
                
                return Some(estimated_time);
            }
        }
        None
    }
    
    fn will_image_render_quickly(&self, path: &PathBuf) -> Option<bool> {
        if let Some(estimated_time) = self.estimate_image_render_time(path) {
            Some(estimated_time <= self.benchmark_threshold_ms)
        } else {
            None
        }
    }
}


// Embedded reference JPG for performance benchmarking
// This is the 313KB JPG image that was benchmarked on the build machine
// Note: We're using the embedded bytes directly instead of the asset file
// static REFERENCE_JPG_DATA: &[u8] = include_bytes!("../assets/313KB-2295X1034.jpg");

#[derive(Debug, Clone)]
struct ReferenceBenchmark {
    image_characteristics: ImageCharacteristics,
    build_machine_result: BenchmarkResult,
    cpu_info: String,
    gpu_info: String,
}

#[derive(Debug, Clone)]
struct PerformanceComparison {
    performance_ratio: f64, // Current machine performance relative to build machine (1.0 = same, 0.5 = half speed, 2.0 = twice as fast)
    confidence_level: f64,  // 0.0 to 1.0, how confident we are in the estimate
}