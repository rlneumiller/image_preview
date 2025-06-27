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

struct ImageViewerApp {
    image_paths: Vec<PathBuf>,
    selected_image_index: Option<usize>,
    image_texture: Option<TextureHandle>,
    status_text: String,
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
        }
    }
}

impl eframe::App for ImageViewerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
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
                            ui.label(&self.status_text);
                        }
                    });
                });
            });
        });

        self.handle_keyboard_nav(ctx);
    }
}

impl ImageViewerApp {
    const MAX_TEXTURE_SIZE: u32 = 16384;

    fn scale_image_if_needed(img: image::DynamicImage) -> image::DynamicImage {
        let (width, height) = (img.width(), img.height());
        
        if width <= Self::MAX_TEXTURE_SIZE && height <= Self::MAX_TEXTURE_SIZE {
            return img;
        }

        // Calculate scale factor to fit within MAX_TEXTURE_SIZE
        let scale_factor = (Self::MAX_TEXTURE_SIZE as f32 / width.max(height) as f32).min(1.0);
        let new_width = (width as f32 * scale_factor) as u32;
        let new_height = (height as f32 * scale_factor) as u32;

        img.resize(new_width, new_height, image::imageops::FilterType::Lanczos3)
    }

    fn load_selected_image(&mut self, ctx: &egui::Context) {
        if let Some(index) = self.selected_image_index {
            if let Some(path) = self.image_paths.get(index) {
                let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("");
                if extension == "svg" {
                    match std::fs::read(path) {
                        Ok(svg_bytes) => {
                            let mut fontdb = usvg::fontdb::Database::new();
                            fontdb.load_system_fonts();
                            
                            let options = usvg::Options {
                                fontdb: Arc::new(fontdb),
                                ..Default::default()
                            };
                            
                            match usvg::Tree::from_data(&svg_bytes, &options) {
                                Ok(usvg_tree) => {
                                    let original_size = usvg_tree.size().to_int_size();
                                    let mut pixmap_size = original_size;
                                    
                                    // Scale down SVG if it's too large for texture
                                    let scale_factor = if original_size.width() > Self::MAX_TEXTURE_SIZE || original_size.height() > Self::MAX_TEXTURE_SIZE {
                                        (Self::MAX_TEXTURE_SIZE as f32 / original_size.width().max(original_size.height()) as f32).min(1.0)
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
                                    if scale_factor < 1.0 {
                                        self.status_text = format!(
                                            "Loaded SVG (scaled from {}x{} to {}x{}): {}", 
                                            original_size.width(), original_size.height(),
                                            pixmap_size.width(), pixmap_size.height(),
                                            path.to_string_lossy()
                                        );
                                    } else {
                                        self.status_text = format!("Loaded SVG: {}", path.to_string_lossy());
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
                                let scaled_img = Self::scale_image_if_needed(img);
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