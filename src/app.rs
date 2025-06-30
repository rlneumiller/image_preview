//! Main application UI and logic

use std::path::PathBuf;
use std::time::Instant;
use eframe::egui;
use egui::TextureHandle;
use glob::glob;

use crate::settings::ImageLoadingSettings;
use crate::benchmark::{PerformanceProfile, SystemPerformanceCategory, run_simple_cpu_benchmark};
use crate::file_locality::FileInfo;
use crate::image_processing::{should_skip_large_file, load_svg_image, load_raster_image, estimate_image_render_time};
use crate::icons::IconRenderer;

pub struct ImageViewerApp {
    pub file_infos: Vec<FileInfo>,
    pub selected_image_index: Option<usize>,
    pub image_texture: Option<TextureHandle>,
    pub status_text: String,
    pub settings: ImageLoadingSettings,
    pub show_settings: bool,
    pub performance_profile: PerformanceProfile,
    pub show_benchmark_window: bool,
    pub benchmark_in_progress: bool,
    pub benchmark_threshold_ms: f64,
    pub run_benchmark_trigger: bool,
    pub auto_benchmark_on_startup: bool,
    // New fields for user confirmation dialog
    pub show_slow_image_dialog: bool,
    pub pending_slow_image_path: Option<PathBuf>,
    pub pending_slow_image_estimated_time: f64,
    // File download-specific fields
    pub show_download_dialog: bool,
    pub pending_download_file: Option<FileInfo>,
    // Icon renderer
    pub icon_renderer: IconRenderer,
}

impl Default for ImageViewerApp {
    fn default() -> Self {
        let mut file_infos = vec![];
        let settings = ImageLoadingSettings::default();
        for ext in settings.supported_formats.iter() {
            if let Ok(paths) = glob(&format!("*.{}", ext)) {
                for entry in paths.flatten() {
                    file_infos.push(FileInfo::new(entry));
                }
            }
        }

        Self {
            file_infos,
            selected_image_index: None,
            image_texture: None,
            status_text: "Select an image".to_string(),
            settings,
            show_settings: false,
            performance_profile: PerformanceProfile::default(),
            show_benchmark_window: false,
            benchmark_in_progress: false,
            benchmark_threshold_ms: 2000.0, // 2 seconds
            run_benchmark_trigger: false,
            auto_benchmark_on_startup: false, // Disabled by default to avoid OneDrive issues
            show_slow_image_dialog: false,
            pending_slow_image_path: None,
            pending_slow_image_estimated_time: 0.0,
            show_download_dialog: false,
            pending_download_file: None,
            icon_renderer: IconRenderer::new(),
        }
    }
}

impl eframe::App for ImageViewerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.render_top_menu(ctx);
        self.render_settings_window(ctx);
        self.render_benchmark_window(ctx);
        self.render_main_panel(ctx);
        self.handle_keyboard_nav(ctx);
        self.handle_benchmark_trigger(ctx);
        self.handle_dialogs(ctx);
    }
}

impl ImageViewerApp {
    /// Update the locality status of a file after it has been accessed/downloaded
    fn update_file_locality_status(&mut self, file_path: &PathBuf) {
        if let Some(file_info) = self.file_infos.iter_mut().find(|f| f.path == *file_path) {
            let new_status = crate::file_locality::get_file_locality_status(file_path);
            if file_info.locality_status != new_status {
                // Clear estimated download size if the file is now local
                let is_now_local = matches!(new_status, crate::file_locality::FileLocalityStatus::Local);
                file_info.locality_status = new_status;
                if is_now_local {
                    file_info.estimated_download_size = None;
                }
            }
        }
    }

    /// Refresh locality status for all files (useful if OneDrive has synced files in background)
    pub fn refresh_all_file_locality_status(&mut self) {
        for file_info in &mut self.file_infos {
            let new_status = crate::file_locality::get_file_locality_status(&file_info.path);
            if file_info.locality_status != new_status {
                // Clear estimated download size if the file is now local
                let is_now_local = matches!(new_status, crate::file_locality::FileLocalityStatus::Local);
                let is_now_on_demand = matches!(new_status, crate::file_locality::FileLocalityStatus::OnDemand);
                file_info.locality_status = new_status;
                if is_now_local {
                    file_info.estimated_download_size = None;
                } else if is_now_on_demand {
                    // Re-calculate download size for on-demand files
                    file_info.estimated_download_size = std::fs::metadata(&file_info.path).ok().map(|m| m.len());
                }
            }
        }
    }

    fn render_top_menu(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("Settings", |ui| {
                    if ui.button("Image Loading Settings").clicked() {
                        self.show_settings = !self.show_settings;
                    }
                    if ui.button("Refresh File Status").clicked() {
                        self.refresh_all_file_locality_status();
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
    }

    fn render_settings_window(&mut self, ctx: &egui::Context) {
        if self.show_settings {
            egui::Window::new("Image Loading Settings")
                .open(&mut self.show_settings)
                .show(ctx, |ui| {
                    ui.checkbox(&mut self.settings.skip_large_images, "Skip very large images");
                    ui.checkbox(&mut self.settings.auto_scale_large_images, "Auto-scale large images");
                    ui.checkbox(&mut self.settings.auto_scale_to_fit, "Scale images to fit display");
                    
                    if self.settings.skip_large_images {
                        self.settings.auto_scale_large_images = false;
                    } else if self.settings.auto_scale_large_images {
                        self.settings.skip_large_images = false;
                    }

                    ui.separator();
                    
                    ui.heading("File Size Limits");
                    
                    // Show current effective limit (whether manual or dynamic)
                    let effective_limit = self.settings.get_effective_max_file_size_mb().unwrap_or(0);
                    let dynamic_limit = crate::settings::ImageLoadingSettings::calculate_dynamic_max_file_size_mb();
                    
                    ui.horizontal(|ui| {
                        ui.label("Current limit:");
                        if self.settings.max_file_size_mb.is_some() {
                            ui.colored_label(egui::Color32::LIGHT_BLUE, format!("{} MB (manual)", effective_limit));
                        } else {
                            ui.colored_label(egui::Color32::LIGHT_GREEN, format!("{} MB (dynamic)", effective_limit));
                        }
                    });
                    
                    ui.horizontal(|ui| {
                        ui.label("Dynamic recommendation:");
                        ui.colored_label(egui::Color32::GRAY, format!("{} MB (based on available RAM)", dynamic_limit));
                    });
                    
                    ui.horizontal(|ui| {
                        ui.label("Manual override (MB):");
                        let mut max_size = self.settings.max_file_size_mb.unwrap_or(0);
                        if ui.add(egui::Slider::new(&mut max_size, 1..=2048)).changed() {
                            self.settings.max_file_size_mb = if max_size > 0 { Some(max_size) } else { None };
                        }
                        if ui.button("Use Dynamic").clicked() {
                            self.settings.max_file_size_mb = None;
                        }
                    });
                    
                    // Show explanation
                    ui.label("💡 Dynamic limit is calculated as 90% of available system RAM");
                    if self.settings.max_file_size_mb.is_none() {
                        ui.colored_label(egui::Color32::LIGHT_GREEN, "✓ Using dynamic calculation - adjusts automatically based on system memory");
                    } else {
                        ui.colored_label(egui::Color32::YELLOW, "⚠ Using manual override - consider using dynamic for better memory management");
                    }

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
                    
                    ui.separator();
                    ui.heading("Debug Options");
                    ui.checkbox(&mut self.settings.debug_file_locality_detection, "Debug file locality detection");
                    
                    ui.separator();
                    ui.heading("Filename Display");
                    ui.checkbox(&mut self.settings.truncate_long_filenames, "Truncate long filenames");
                    
                    if self.settings.truncate_long_filenames {
                        ui.horizontal(|ui| {
                            ui.label("Max length:");
                            ui.add(egui::Slider::new(&mut self.settings.max_filename_length, 20..=100));
                        });
                        
                        ui.horizontal(|ui| {
                            ui.label("Style:");
                            egui::ComboBox::from_id_source("truncation_style")
                                .selected_text(match self.settings.truncation_style {
                                    crate::settings::FilenameTruncationStyle::None => "None",
                                    crate::settings::FilenameTruncationStyle::Ellipsis => "Ellipsis (…)",
                                    crate::settings::FilenameTruncationStyle::FadeEnd => "Fade End",
                                })
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut self.settings.truncation_style, 
                                        crate::settings::FilenameTruncationStyle::None, "None");
                                    ui.selectable_value(&mut self.settings.truncation_style, 
                                        crate::settings::FilenameTruncationStyle::Ellipsis, "Ellipsis (…)");
                                    ui.selectable_value(&mut self.settings.truncation_style, 
                                        crate::settings::FilenameTruncationStyle::FadeEnd, "Fade End");
                                });
                        });
                        
                        if self.settings.truncation_style != crate::settings::FilenameTruncationStyle::None {
                            ui.horizontal(|ui| {
                                ui.label("Ellipsis:");
                                ui.add(egui::TextEdit::singleline(&mut self.settings.ellipsis_char).desired_width(40.0));
                                if ui.button("…").clicked() {
                                    self.settings.ellipsis_char = "…".to_string();
                                }
                                if ui.button("...").clicked() {
                                    self.settings.ellipsis_char = "...".to_string();
                                }
                                if ui.button("..").clicked() {
                                    self.settings.ellipsis_char = "..".to_string();
                                }
                            });
                        }
                        
                        // Preview of truncation
                        ui.horizontal(|ui| {
                            ui.label("Preview:");
                            let sample_filename = "very_long_filename_example_that_would_be_truncated.jpg";
                            let truncated = self.settings.truncate_filename(sample_filename);
                            ui.code(&truncated);
                        });
                    }
                });
        }
    }

    fn render_benchmark_window(&mut self, ctx: &egui::Context) {
        if !self.show_benchmark_window {
            return;
        }

        let mut show_window = true;
        let mut run_benchmark_clicked = false;
        
        egui::Window::new("Performance Benchmark")
            .open(&mut show_window)
            .default_width(500.0)
            .show(ctx, |ui| {
                ui.heading("Benchmark Configuration");
                
                ui.horizontal(|ui| {
                    ui.label("Performance threshold (ms):");
                    ui.add(egui::Slider::new(&mut self.benchmark_threshold_ms, 100.0..=10000.0));
                });
                
                ui.separator();
                
                if self.benchmark_in_progress {
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
                                let (icon_name, color) = if result.success { 
                                    ("circle-check", egui::Color32::GREEN)
                                } else { 
                                    ("x", egui::Color32::RED)
                                };
                                
                                ui.horizontal(|ui| {
                                    self.icon_renderer.icon_label(ui, ctx, icon_name, 16.0, color);
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
        
        if run_benchmark_clicked {
            self.run_benchmark_trigger = true;
        }
    }

    fn render_main_panel(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            self.render_file_list(ui, ctx);
            self.render_image_display(ui);
        });
    }

    fn render_file_list(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        egui::SidePanel::left("image_list_panel")
            .resizable(true)
            .show_inside(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.heading("Images");
                    let mut changed = false;
                    for (index, file_info) in self.file_infos.iter().enumerate() {
                        let is_selected = self.selected_image_index == Some(index);
                        
                        // Pre-calculate performance info to avoid borrowing issues
                        let has_benchmark_data = !self.performance_profile.benchmark_results.is_empty();
                        let performance_info = if has_benchmark_data && !file_info.will_trigger_download() {
                            // Only calculate performance for locally available files to avoid triggering downloads
                            self.will_image_render_quickly(&file_info.path)
                        } else {
                            None
                        };
                        let estimated_time = if has_benchmark_data && !file_info.will_trigger_download() {
                            // Only estimate time for locally available files to avoid triggering downloads
                            estimate_image_render_time(&file_info.path, &self.performance_profile)
                        } else {
                            None
                        };
                        
                        ui.horizontal(|ui| {
                            // Show file locality status indicator
                            let locality_color = match file_info.locality_status {
                                crate::file_locality::FileLocalityStatus::Local => egui::Color32::GREEN,
                                crate::file_locality::FileLocalityStatus::OnDemand => egui::Color32::LIGHT_BLUE,
                                crate::file_locality::FileLocalityStatus::Unknown => egui::Color32::GRAY,
                            };
                            self.icon_renderer.icon_label(ui, ctx, file_info.locality_status.icon(), 16.0, locality_color)
                                .on_hover_text(format!(
                                    "{}\n{}",
                                    file_info.locality_status.description(),
                                    if file_info.will_trigger_download() {
                                        if let Some(size) = file_info.estimated_download_size {
                                            format!("Download size: {:.1} MB", size as f64 / (1024.0 * 1024.0))
                                        } else {
                                            "Will trigger download".to_string()
                                        }
                                    } else {
                                        "Safe for immediate access".to_string()
                                    }
                                ));
                            
                            // Show performance indicator if benchmark data is available
                            if has_benchmark_data {
                                if file_info.will_trigger_download() {
                                    // Special indicator for files requiring download
                                    self.icon_renderer.icon_label(ui, ctx, "cloud", 16.0, egui::Color32::LIGHT_BLUE).on_hover_text("Remote file - performance estimate unavailable until downloaded");
                                } else if let Some(will_be_fast) = performance_info {
                                    let (icon, color) = if will_be_fast { 
                                        ("circle-check", egui::Color32::GREEN)
                                    } else { 
                                        ("clock", egui::Color32::YELLOW)
                                    };
                                    let tooltip = if will_be_fast { 
                                        "Expected to render quickly" 
                                    } else { 
                                        "May take longer to render" 
                                    };
                                    self.icon_renderer.icon_label(ui, ctx, icon, 16.0, color).on_hover_text(tooltip);
                                } else {
                                    self.icon_renderer.icon_label(ui, ctx, "help", 16.0, egui::Color32::GRAY).on_hover_text("Performance unknown");
                                }
                            }
                            
                            let filename = file_info.path.file_name()
                                .map(|f| f.to_string_lossy().to_string())
                                .unwrap_or_else(|| file_info.path.to_string_lossy().to_string());
                            
                            let display_filename = self.settings.truncate_filename(&filename);
                            let label = ui.selectable_label(is_selected, display_filename);
                            
                            if label.clicked() {
                                self.selected_image_index = Some(index);
                                changed = true;
                            }
                            
                            // Combine tooltips for full filename and render time
                            let mut tooltip_parts = Vec::new();
                            
                            if let Some(filename_tooltip) = self.settings.get_full_filename_tooltip(&file_info.path) {
                                tooltip_parts.push(filename_tooltip);
                            }
                            
                            if let Some(time) = estimated_time {
                                tooltip_parts.push(format!("Estimated render time: {:.0}ms", time));
                            }
                            
                            if !tooltip_parts.is_empty() {
                                label.on_hover_text(tooltip_parts.join("\n"));
                            }
                        });
                    }
                    if changed {
                        self.load_selected_image(ctx);
                    }
                });
            });
    }

    fn render_image_display(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show_inside(ui, |ui| {
            // Set a neutral grey background for the image preview area
            ui.style_mut().visuals.extreme_bg_color = egui::Color32::from_gray(128);
            let frame = egui::Frame::default()
                .fill(egui::Color32::from_gray(128))
                .inner_margin(egui::Margin::same(10.0));
            
            frame.show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    if let Some(texture) = &self.image_texture {
                        if self.settings.auto_scale_to_fit {
                            // Calculate available space for the image
                            let available_size = ui.available_size();
                            let texture_size = texture.size_vec2();
                            
                            // Calculate scale factor to fit image within available space
                            let scale_x = available_size.x / texture_size.x;
                            let scale_y = available_size.y / texture_size.y;
                            let scale = scale_x.min(scale_y).min(1.0); // Don't scale up, only down
                            
                            let scaled_size = texture_size * scale;
                            ui.image((texture.id(), scaled_size));
                        } else {
                            ui.image(texture);
                        }
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
    }

    fn handle_keyboard_nav(&mut self, ctx: &egui::Context) {
        let mut changed = false;
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
            if let Some(selected_index) = self.selected_image_index {
                if selected_index > 0 {
                    self.selected_image_index = Some(selected_index - 1);
                    changed = true;
                }
            } else if !self.file_infos.is_empty() {
                self.selected_image_index = Some(self.file_infos.len() - 1);
                changed = true;
            }
        }

        if ctx.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
            if let Some(selected_index) = self.selected_image_index {
                if selected_index < self.file_infos.len() - 1 {
                    self.selected_image_index = Some(selected_index + 1);
                    changed = true;
                }
            } else if !self.file_infos.is_empty() {
                self.selected_image_index = Some(0);
                changed = true;
            }
        }

        if changed {
            self.load_selected_image(ctx);
        }
    }

    fn handle_benchmark_trigger(&mut self, ctx: &egui::Context) {
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
    }

    fn handle_dialogs(&mut self, ctx: &egui::Context) {
        self.handle_slow_image_dialog(ctx);
        self.handle_download_dialog(ctx);
    }

    fn handle_slow_image_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_slow_image_dialog {
            return;
        }

        let mut load_anyway = false;
        
        egui::Window::new("Slow Image Warning")
            .open(&mut self.show_slow_image_dialog)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.horizontal(|ui| {
                        self.icon_renderer.icon_label(ui, ctx, "alert-triangle", 16.0, egui::Color32::YELLOW);
                        ui.label("Performance Warning");
                    });
                    ui.separator();
                    
                    if let Some(ref path) = self.pending_slow_image_path {
                        let filename = path.file_name()
                            .map(|f| f.to_string_lossy().to_string())
                            .unwrap_or_else(|| path.to_string_lossy().to_string());
                        let display_filename = self.settings.truncate_filename(&filename);
                        ui.label(format!("Image: {}", display_filename));
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
                    
                    ui.vertical_centered(|ui| {
                        if ui.button("Load Anyway").clicked() {
                            load_anyway = true;
                        }
                    });
                });
            });
        
        if !self.show_slow_image_dialog {
            self.pending_slow_image_path = None;
            self.pending_slow_image_estimated_time = 0.0;
        } else if load_anyway {
            self.show_slow_image_dialog = false;
            if let Some(path) = self.pending_slow_image_path.take() {
                // Find the index and load the image
                if let Some(index) = self.file_infos.iter().position(|f| f.path == path) {
                    self.selected_image_index = Some(index);
                    self.force_load_selected_image(ctx);
                }
            }
            self.pending_slow_image_estimated_time = 0.0;
        }
    }

    fn handle_download_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_download_dialog {
            return;
        }

        let mut download_anyway = false;
        
        egui::Window::new("File Download Warning")
            .open(&mut self.show_download_dialog)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.horizontal(|ui| {
                        self.icon_renderer.icon_label(ui, ctx, "cloud", 16.0, egui::Color32::LIGHT_BLUE);
                        self.icon_renderer.icon_label(ui, ctx, "download", 16.0, egui::Color32::LIGHT_BLUE);
                        ui.label("File Download Required");
                    });
                    ui.separator();
                    
                    if let Some(ref file_info) = self.pending_download_file {
                        let filename = file_info.path.file_name()
                            .map(|f| f.to_string_lossy().to_string())
                            .unwrap_or_else(|| file_info.path.to_string_lossy().to_string());
                        let display_filename = self.settings.truncate_filename(&filename);
                        ui.label(format!("File: {}", display_filename));
                        ui.label(format!("Status: {}", file_info.locality_status.description()));
                        
                        if let Some(size) = file_info.estimated_download_size {
                            ui.label(format!("Download size: {:.1} MB", size as f64 / (1024.0 * 1024.0)));
                        }
                    }
                    
                    ui.separator();
                    ui.label("This file is stored remotely and needs to be downloaded");
                    ui.label("before it can be viewed. This may take some time depending");
                    ui.label("on your internet connection.");
                    
                    ui.separator();
                    
                    ui.vertical_centered(|ui| {
                        if ui.button("Download and Open").clicked() {
                            download_anyway = true;
                        }
                    });
                });
            });
        
        if !self.show_download_dialog {
            self.pending_download_file = None;
        } else if download_anyway {
            self.show_download_dialog = false;
            if let Some(file_info) = self.pending_download_file.take() {
                // Find the index and load the image (this will trigger download)
                if let Some(index) = self.file_infos.iter().position(|f| f.path == file_info.path) {
                    self.selected_image_index = Some(index);
                    self.force_load_selected_image(ctx);
                }
            }
        }
    }

    pub fn load_selected_image(&mut self, ctx: &egui::Context) {
        if let Some(index) = self.selected_image_index {
            if let Some(file_info) = self.file_infos.get(index) {
                // Check if this is a file that will trigger download
                if file_info.will_trigger_download() {
                    // Show download warning dialog
                    self.pending_download_file = Some(file_info.clone());
                    self.show_download_dialog = true;
                    return; // Don't load immediately, wait for user confirmation
                }
                
                // Check if we should prompt user for slow images (only if benchmark data is available)
                if !self.performance_profile.benchmark_results.is_empty() {
                    if let Some(estimated_time) = estimate_image_render_time(&file_info.path, &self.performance_profile) {
                        if estimated_time > self.benchmark_threshold_ms {
                            // Show slow image warning dialog
                            self.pending_slow_image_path = Some(file_info.path.clone());
                            self.pending_slow_image_estimated_time = estimated_time;
                            self.show_slow_image_dialog = true;
                            return; // Don't load immediately, wait for user confirmation
                        }
                    }
                }
                
                // If we get here, either no OneDrive/benchmark issues, or user confirmed
                self.force_load_selected_image(ctx);
            }
        }
    }

    pub fn force_load_selected_image(&mut self, ctx: &egui::Context) {
        if let Some(index) = self.selected_image_index {
            if let Some(file_info) = self.file_infos.get(index) {
                let path = file_info.path.clone(); // Clone the path to avoid borrowing issues
                
                // Check file size first (but allow on-demand files when forcing)
                if let Some(skip_message) = should_skip_large_file(&path, &self.settings, true) {
                    self.status_text = skip_message;
                    self.image_texture = None;
                    return;
                }

                let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("");
                
                let result = if extension == "svg" {
                    load_svg_image(&path, &self.settings, ctx, true)
                } else {
                    load_raster_image(&path, &self.settings, ctx, true)
                };

                match result {
                    Ok(texture) => {
                        self.image_texture = Some(texture);
                        let recolor_suffix = if extension == "svg" && self.settings.svg_recolor_enabled {
                            " (recolored)"
                        } else {
                            ""
                        };
                        let filename = path.file_name()
                            .map(|f| f.to_string_lossy().to_string())
                            .unwrap_or_else(|| path.to_string_lossy().to_string());
                        let display_filename = self.settings.truncate_filename(&filename);
                        self.status_text = format!("Loaded: {}{}", display_filename, recolor_suffix);
                        
                        // Update file locality status after successful load (in case it was downloaded)
                        self.update_file_locality_status(&path);
                    }
                    Err(e) => {
                        self.image_texture = None;
                        let filename = path.file_name()
                            .map(|f| f.to_string_lossy().to_string())
                            .unwrap_or_else(|| path.to_string_lossy().to_string());
                        let display_filename = self.settings.truncate_filename(&filename);
                        self.status_text = format!("Error loading {}: {}", display_filename, e);
                    }
                }
            }
        }
    }

    pub fn run_benchmark(&mut self, ctx: &egui::Context) {
        if self.benchmark_in_progress {
            return;
        }
        
        self.benchmark_in_progress = true;
        self.performance_profile.benchmark_results.clear();
        self.performance_profile.last_benchmark_time = Some(Instant::now());
        
        // Run safe benchmarks using existing images
        let results = self.performance_profile.benchmark_safe_images(ctx);
        
        self.benchmark_in_progress = false;
        
        // Update status
        let successful_count = results.iter().filter(|r| r.success).count();
        let total_count = results.len();
        
        self.status_text = format!(
            "Benchmark completed: {}/{} images processed successfully", 
            successful_count, total_count
        );
    }

    fn will_image_render_quickly(&self, path: &PathBuf) -> Option<bool> {
        if let Some(estimated_time) = estimate_image_render_time(path, &self.performance_profile) {
            Some(estimated_time <= self.benchmark_threshold_ms)
        } else {
            None
        }
    }
}
