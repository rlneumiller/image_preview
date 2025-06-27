#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use eframe::egui;
use egui::{ColorImage, TextureHandle};
use glob::glob;
use image::ImageReader;
use resvg::{tiny_skia, usvg};
use std::path::PathBuf;
use std::sync::Arc;

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
}

impl Default for ImageViewerApp {
    fn default() -> Self {
        let mut image_paths = vec![];
        let extensions = ["png", "jpg", "jpeg", "svg", "bmp"];
        for ext in extensions.iter() {
            if let Ok(paths) = glob(&format!("*.{}", ext)) {
                for entry in paths {
                    if let Ok(path) = entry {
                        image_paths.push(path);
                    }
                }
            }
        }

        Self {
            image_paths,
            selected_image_index: None,
            image_texture: None,
            status_text: "Welcome to the Image Viewer!".to_string(),
            settings: ImageLoadingSettings::default(),
            show_settings: false,
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

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::SidePanel::left("image_list_panel")
                .resizable(true)
                .show_inside(ui, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        ui.heading("Images");
                        let mut changed = false;
                        for (index, path) in self.image_paths.iter().enumerate() {
                            let is_selected = self.selected_image_index == Some(index);
                            let label = ui.selectable_label(is_selected, path.to_string_lossy());
                            if label.clicked() {
                                self.selected_image_index = Some(index);
                                changed = true;
                            }
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
}