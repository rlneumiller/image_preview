#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use eframe::egui;
use egui::{ColorImage, TextureHandle};
use glob::glob;
use image::ImageReader;
use std::path::PathBuf;

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([800.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Image Viewer",
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
                ui.vertical_centered(|ui| {
                    if let Some(texture) = &self.image_texture {
                        ui.image(texture);
                    } else {
                        ui.label(&self.status_text);
                    }
                });
            });
        });

        self.handle_keyboard_nav(ctx);
    }
}

impl ImageViewerApp {
    fn load_selected_image(&mut self, ctx: &egui::Context) {
        if let Some(index) = self.selected_image_index {
            if let Some(path) = self.image_paths.get(index) {
                match ImageReader::open(path) {
                    Ok(reader) => match reader.decode() {
                        Ok(img) => {
                            let size = [img.width() as _, img.height() as _];
                            let rgba = img.to_rgba8();
                            let pixels = rgba.as_flat_samples();
                            let color_image = ColorImage::from_rgba_unmultiplied(size, pixels.as_slice());
                            self.image_texture = Some(ctx.load_texture(
                                path.to_string_lossy(),
                                color_image,
                                Default::default(),
                            ));
                            self.status_text = format!("Loaded: {}", path.to_string_lossy());
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