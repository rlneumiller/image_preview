// Test file to demonstrate icon loading failure scenarios

use std::path::PathBuf;

fn main() {
    println!("Testing icon failure scenarios:");
    
    // Scenario 1: Missing icon file
    println!("1. Missing icon: 'nonexistent-icon'");
    // This would fail because 'nonexistent-icon.svg' doesn't exist
    
    // Scenario 2: Invalid size
    println!("2. Invalid size: 0.0");
    // This would fail because pixmap creation with size 0 fails
    
    // Scenario 3: Corrupted SVG content
    println!("3. Corrupted SVG");
    // If an SVG file contained invalid XML, parsing would fail
    
    // Current icons that should work:
    let available_icons = vec![
        "alert-triangle",
        "check", 
        "circle-check",
        "clock",
        "cloud",
        "device-floppy", 
        "download",
        "help",
        "x"
    ];
    
    println!("Available icons: {:?}", available_icons);
    
    // Test with a typo (common failure case)
    println!("4. Typo in icon name: 'chekc' instead of 'check'");
}
