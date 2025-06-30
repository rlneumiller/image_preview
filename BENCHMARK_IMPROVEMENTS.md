# Benchmark System Improvements

## Overview

The benchmark system has been simplified to eliminate the complexity of carrying around a reference JPG file and instead uses intelligent criteria to safely test with any images found in the workspace.

## Key Changes

- Uses existing images in the workspace for benchmarking
- No embedded binary data
- Tests with real images the user actually has

### 2. Implemented Intelligent Safety Criteria

The new system uses performance-based limits to minimize locking up the UI (**TODO: async**):

#### Performance Categories & Safe Limits (circa 2025 - **TODO: These need to be verified**)

| Category | Max File Size | Max Megapixels | Max Test Images | Typical Hardware |
|----------|---------------|----------------|-----------------|------------------|
| Low Power | 2MB | 4MP (2048×2048) | 3 images | Old/low-power systems |
| Moderate | 5MB | 8MP (~2800×2800) | 5 images | Typical laptops |
| Good | 10MB | 16MP (4096×4096) | 8 images | Modern laptops |
| High | 20MB | 32MP (~5600×5600) | 10 images | High-end desktops |
| Excellent | 50MB | 64MP (8192×8192) | 15 images | Top-tier systems |

### 3. SMRT<sup>™</sup> Image Selection

The benchmarking system now:

1. **Scans for available images** in assets folder first, then current directory
2. **Filters by safety criteria** based on system performance category
3. **Excludes files that would trigger downloads**
4. **Sorts by file size** (smallest first for safer testing)
5. **Limits test count** to prevent excessive benchmarking time

### 4. Prevents UI Hanging

#### Size-Based Filtering
- Pre-filters images by file size before attempting to load
- Checks image dimensions using efficient metadata reading
- Only tests images within safe megapixel limits

#### Performance-Aware Limits
- Automatically determines system performance category
- Adjusts limits based on detected hardware capabilities
- Conservative approach for unknown/low-power systems

#### Progressive Testing
- Starts with smallest images first
- Stops at predetermined safe limits
- Provides meaningful feedback even with partial results

## Benefits

### 1. **Less Bloat**
- Removed embedded test image
- Cleaner codebase without hardcoded assets
- More maintainable benchmark system

### 2. **Real-World Testing**
- Tests with actual user images
- More representative performance data
- Better estimates for actual usage patterns

### 3. **UI Responsiveness**
- Prevents testing of gargantuan images that could hang the UI
- Conservative limits based on system capabilities
- Smart filtering before attempting to load images

### 4. **Adaptive Behavior**
- Automatically adjusts to system performance
- Safe for low-power devices, hopefully
- Takes advantage of high-performance systems

## Technical Implementation

### Core Functions

#### `find_safe_benchmark_images(limits: &BenchmarkLimits) -> Vec<PathBuf>`
- Intelligently finds images suitable for benchmarking
- Applies size and dimension filtering
- Avoids images that would trigger on-demand downloads

#### `SystemPerformanceCategory::safe_benchmark_limits()`
- Returns appropriate limits for each performance tier
- Conservative approach for system protection
- Scalable limits for better hardware

#### `PerformanceProfile::benchmark_safe_images()`
- Replaces the old reference image benchmarking
- Uses multiple real images for better data
- Provides comprehensive performance profiling

### Safety Mechanisms

1. **Pre-flight Checks**: File size and metadata checking before image loading
2. **Dimension Limits**: Megapixel-based filtering to prevent memory issues
3. **Count Limits**: Maximum number of test images per performance category
4. **On-demand Downloads Awareness**: Skips cloud-only files to avoid unwanted downloads

## Usage

The benchmark system works automatically:

1. **System Detection**: Determines performance category on startup
2. **Safe Image Discovery**: Finds appropriate test images
3. **Progressive Testing**: Tests images from smallest to largest
4. **Performance Profiling**: Builds accurate performance estimates

Users benefit from:
- No hanging UI when encountering large images
- Accurate performance warnings for slow images
- Smooth experience across different hardware tiers
- Automatic optimization without manual configuration

## Future Improvements

Potential enhancements:
- Consider asynchronous benchmarking to prevent UI blocking
- Dynamic limit adjustment based on available RAM
- Format-specific optimization (JPEG vs PNG vs BMP)
- Background benchmarking for discovered images
- Machine learning-based performance prediction
