//! Image loading and processing functionality

use std::path::PathBuf;
use eframe::egui;
use egui::{ColorImage, TextureHandle};
use image::ImageReader;
use resvg::{tiny_skia, usvg};
use regex;

use crate::settings::ImageLoadingSettings;
use crate::onedrive::FileInfo;
use crate::benchmark::ImageCharacteristics;

pub fn should_skip_large_file(path: &PathBuf, settings: &ImageLoadingSettings) -> Option<String> {
    // Check OneDrive status first to avoid any potential file access issues
    let file_info = FileInfo::new(path.clone());
    if file_info.will_trigger_download() {
        return Some(format!(
            "Skipped OneDrive on-demand file: {}", 
            path.to_string_lossy()
        ));
    }
    
    if let Some(max_mb) = settings.max_file_size_mb {
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

pub fn scale_image_if_needed(img: image::DynamicImage, settings: &ImageLoadingSettings) -> Result<image::DynamicImage, String> {
    // With max_texture_size removed, we'll only scale if auto_scale_large_images is enabled
    // and the image is considered "large" (using a reasonable default threshold)
    let (width, height) = (img.width(), img.height());
    
    // Use a reasonable default threshold for "large" images (e.g., 8192x8192)
    const LARGE_IMAGE_THRESHOLD: u32 = 8192;
    
    if width <= LARGE_IMAGE_THRESHOLD && height <= LARGE_IMAGE_THRESHOLD {
        return Ok(img);
    }

    if settings.skip_large_images {
        return Err(format!(
            "Image too large ({}x{} > {}x{} threshold)", 
            width, height, LARGE_IMAGE_THRESHOLD, LARGE_IMAGE_THRESHOLD
        ));
    }

    if settings.auto_scale_large_images {
        // Calculate scale factor to fit within threshold
        let scale_factor = (LARGE_IMAGE_THRESHOLD as f32 / width.max(height) as f32).min(1.0);
        let new_width = (width as f32 * scale_factor) as u32;
        let new_height = (height as f32 * scale_factor) as u32;

        Ok(img.resize(new_width, new_height, image::imageops::FilterType::Lanczos3))
    } else {
        Err(format!(
            "Image too large ({}x{} > {}x{} threshold) and auto-scaling disabled", 
            width, height, LARGE_IMAGE_THRESHOLD, LARGE_IMAGE_THRESHOLD
        ))
    }
}

pub fn recolor_svg_simple(svg_content: &str, settings: &ImageLoadingSettings) -> String {
    if !settings.svg_recolor_enabled {
        return svg_content.to_string();
    }

    let target_hex = format!(
        "#{:02x}{:02x}{:02x}",
        settings.svg_target_color[0],
        settings.svg_target_color[1],
        settings.svg_target_color[2]
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

pub fn load_svg_image(path: &PathBuf, settings: &ImageLoadingSettings, ctx: &egui::Context) -> Result<TextureHandle, String> {
    // Check OneDrive status first to avoid triggering downloads
    let file_info = FileInfo::new(path.clone());
    if file_info.will_trigger_download() {
        return Err("Cannot load OneDrive on-demand file - would trigger download".to_string());
    }
    
    let svg_content = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read SVG file: {}", e))?;
    
    // Apply recoloring if enabled
    let processed_svg = recolor_svg_simple(&svg_content, settings);
    let svg_bytes = processed_svg.as_bytes();
    
    let mut fontdb = usvg::fontdb::Database::new();
    fontdb.load_system_fonts();
    
    let options = usvg::Options {
        fontdb: std::sync::Arc::new(fontdb),
        ..Default::default()
    };
    
    let tree = usvg::Tree::from_data(svg_bytes, &options)
        .map_err(|e| format!("Failed to parse SVG: {}", e))?;
    
    let size = tree.size();
    let width = size.width() as u32;
    let height = size.height() as u32;
    
    // Apply scaling if needed
    const LARGE_SVG_THRESHOLD: u32 = 8192;
    let (scaled_width, scaled_height) = if width > LARGE_SVG_THRESHOLD || height > LARGE_SVG_THRESHOLD {
        if settings.skip_large_images {
            return Err(format!("SVG too large ({}x{} > {}x{} threshold)", width, height, LARGE_SVG_THRESHOLD, LARGE_SVG_THRESHOLD));
        }
        
        if settings.auto_scale_large_images {
            let scale_factor = (LARGE_SVG_THRESHOLD as f32 / width.max(height) as f32).min(1.0);
            ((width as f32 * scale_factor) as u32, (height as f32 * scale_factor) as u32)
        } else {
            return Err(format!("SVG too large ({}x{} > {}x{} threshold) and auto-scaling disabled", width, height, LARGE_SVG_THRESHOLD, LARGE_SVG_THRESHOLD));
        }
    } else {
        (width, height)
    };
    
    let mut pixmap = tiny_skia::Pixmap::new(scaled_width, scaled_height)
        .ok_or("Failed to create pixmap")?;
    
    let scale_x = scaled_width as f32 / width as f32;
    let scale_y = scaled_height as f32 / height as f32;
    let transform = tiny_skia::Transform::from_scale(scale_x, scale_y);
    
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    
    // Convert to RGBA
    let rgba_data: Vec<u8> = pixmap.data()
        .chunks_exact(4)
        .flat_map(|bgra| [bgra[2], bgra[1], bgra[0], bgra[3]]) // BGRA to RGBA
        .collect();
    
    let color_image = ColorImage::from_rgba_unmultiplied(
        [scaled_width as usize, scaled_height as usize],
        &rgba_data,
    );
    
    let texture_name = format!("svg_{}", path.file_name().unwrap_or_default().to_string_lossy());
    let recolor_suffix = if settings.svg_recolor_enabled { "_recolored" } else { "" };
    
    Ok(ctx.load_texture(
        format!("{}{}", texture_name, recolor_suffix),
        color_image,
        Default::default(),
    ))
}

pub fn load_raster_image(path: &PathBuf, settings: &ImageLoadingSettings, ctx: &egui::Context) -> Result<TextureHandle, String> {
    // Check OneDrive status first to avoid triggering downloads
    let file_info = FileInfo::new(path.clone());
    if file_info.will_trigger_download() {
        return Err("Cannot load OneDrive on-demand file - would trigger download".to_string());
    }
    
    let img = ImageReader::open(path)
        .map_err(|e| format!("Failed to open image: {}", e))?
        .decode()
        .map_err(|e| format!("Failed to decode image: {}", e))?;
    
    // Apply scaling if needed
    let scaled_img = scale_image_if_needed(img, settings)?;
    
    let size = [scaled_img.width() as _, scaled_img.height() as _];
    let rgba = scaled_img.to_rgba8();
    let pixels = rgba.as_flat_samples();
    let color_image = ColorImage::from_rgba_unmultiplied(size, pixels.as_slice());
    
    let texture_name = format!("image_{}", path.file_name().unwrap_or_default().to_string_lossy());
    
    Ok(ctx.load_texture(
        texture_name,
        color_image,
        Default::default(),
    ))
}

pub fn estimate_image_render_time(path: &PathBuf, performance_profile: &crate::benchmark::PerformanceProfile) -> Option<f64> {
    // For OneDrive files, skip dimension detection to avoid triggering downloads
    let file_info = FileInfo::new(path.clone());
    if file_info.will_trigger_download() {
        return None; // Cannot safely estimate without triggering download
    }
    
    // Try to get image dimensions without fully loading (safe for local files only)
    if let Ok(reader) = ImageReader::open(path) {
        if let Ok((width, height)) = reader.into_dimensions() {
            let format = path.extension()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_lowercase();
            
            let characteristics = ImageCharacteristics::new(path, width, height, format);
            let estimated_time = performance_profile.estimate_render_time(&characteristics);
            
            return Some(estimated_time);
        }
    }
    None
}
