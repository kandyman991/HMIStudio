//! B5-07 bounded full pixel decoding gate for imported PNG and JPEG rasters.
//!
//! Structural envelope checks in `resources.rs` support inert evidence
//! retention only. Before any renderer, extractor, or runtime package compiler
//! consumes imported raster bytes, it MUST pass through this module so full
//! decompression, dimension, pixel-count, and decoded-byte limits are enforced.

use crate::resources::{jpeg_dimensions, png_dimensions};
use std::io::Cursor;

/// Maximum compressed input bytes admitted by the default consumer gate (4 MiB).
pub const MAX_RASTER_INPUT_BYTES: usize = 4 * 1024 * 1024;
/// Maximum width or height in pixels admitted by the default consumer gate.
pub const MAX_RASTER_DIMENSION: u32 = 4096;
/// Maximum total pixels (`width * height`) admitted by the default consumer gate.
pub const MAX_RASTER_PIXELS: u64 = 16_000_000;
/// Maximum decoded RGBA8 bytes (`width * height * 4`) admitted by the default gate (64 MiB).
pub const MAX_DECODED_BYTES: usize = 64 * 1024 * 1024;

/// Supported raster formats enforced by the B5-07 consumer gate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RasterFormat {
    Png,
    Jpeg,
}

impl RasterFormat {
    /// Resolve the canonical MIME media type into a supported raster format.
    pub fn from_media_type(media_type: &str) -> Option<Self> {
        match media_type {
            "image/png" => Some(Self::Png),
            "image/jpeg" => Some(Self::Jpeg),
            _ => None,
        }
    }

    /// Canonical MIME media type string.
    pub fn media_type(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
        }
    }
}

/// Explicit resource bounds enforced before and during full pixel decoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RasterLimits {
    pub max_input_bytes: usize,
    pub max_width: u32,
    pub max_height: u32,
    pub max_pixels: u64,
    pub max_decoded_bytes: usize,
}

impl RasterLimits {
    /// Default conservative B5-07 consumer limits.
    pub const DEFAULT: Self = Self {
        max_input_bytes: MAX_RASTER_INPUT_BYTES,
        max_width: MAX_RASTER_DIMENSION,
        max_height: MAX_RASTER_DIMENSION,
        max_pixels: MAX_RASTER_PIXELS,
        max_decoded_bytes: MAX_DECODED_BYTES,
    };
}

impl Default for RasterLimits {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Payload-free refusal reasons for bounded raster decoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RasterError {
    UnsupportedMediaType,
    InputSizeLimit,
    InvalidContainer,
    DimensionLimit,
    PixelCountLimit,
    DecodedByteLimit,
    MalformedCompressedData,
    DimensionMismatch,
}

impl std::fmt::Display for RasterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::UnsupportedMediaType => "Unsupported raster media type",
            Self::InputSizeLimit => "Raster input exceeds compressed byte limit",
            Self::InvalidContainer => "Raster container structure is invalid or unsupported",
            Self::DimensionLimit => "Raster width or height exceeds consumer dimension limits",
            Self::PixelCountLimit => "Raster pixel count exceeds consumer pixel limit",
            Self::DecodedByteLimit => "Raster decoded output exceeds consumer byte limit",
            Self::MalformedCompressedData => "Raster compressed image stream failed full decoding",
            Self::DimensionMismatch => "Decoded raster dimensions do not match declared metadata",
        })
    }
}

impl std::error::Error for RasterError {}

/// Fully decoded, bounds-verified RGBA8 raster buffer.
///
/// `Debug` is intentionally redacted so raw pixel data is never printed into logs.
#[derive(Clone, PartialEq, Eq)]
pub struct DecodedRaster {
    format: RasterFormat,
    width: u32,
    height: u32,
    rgba8: Vec<u8>,
}

impl DecodedRaster {
    pub fn format(&self) -> RasterFormat {
        self.format
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// Fully decoded 8-bit RGBA pixels (`width * height * 4` bytes).
    pub fn rgba8(&self) -> &[u8] {
        &self.rgba8
    }
}

impl std::fmt::Debug for DecodedRaster {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DecodedRaster")
            .field("format", &self.format)
            .field("width", &self.width)
            .field("height", &self.height)
            .field("rgba8_len", &self.rgba8.len())
            .finish()
    }
}

fn check_geometry(
    width: u32,
    height: u32,
    expected: Option<(u32, u32)>,
    limits: &RasterLimits,
) -> Result<usize, RasterError> {
    if width == 0 || height == 0 || width > limits.max_width || height > limits.max_height {
        return Err(RasterError::DimensionLimit);
    }
    if let Some((ew, eh)) = expected {
        if width != ew || height != eh {
            return Err(RasterError::DimensionMismatch);
        }
    }
    let pixels = u64::from(width)
        .checked_mul(u64::from(height))
        .ok_or(RasterError::PixelCountLimit)?;
    if pixels > limits.max_pixels {
        return Err(RasterError::PixelCountLimit);
    }
    let rgba_bytes = usize::try_from(pixels.checked_mul(4).ok_or(RasterError::DecodedByteLimit)?)
        .map_err(|_| RasterError::DecodedByteLimit)?;
    if rgba_bytes > limits.max_decoded_bytes {
        return Err(RasterError::DecodedByteLimit);
    }
    Ok(rgba_bytes)
}

fn decode_png(
    bytes: &[u8],
    expected: Option<(u32, u32)>,
    limits: &RasterLimits,
) -> Result<DecodedRaster, RasterError> {
    let (w, h) = png_dimensions(bytes).ok_or(RasterError::InvalidContainer)?;
    let rgba_bytes = check_geometry(w, h, expected, limits)?;

    // Enforce decoder internal allocation limit before decompressing IDAT.
    let mut decoder = png::Decoder::new_with_limits(
        Cursor::new(bytes),
        png::Limits {
            bytes: limits.max_decoded_bytes,
        },
    );
    decoder.set_transformations(
        png::Transformations::EXPAND | png::Transformations::STRIP_16 | png::Transformations::ALPHA,
    );
    let mut reader = decoder
        .read_info()
        .map_err(|_| RasterError::MalformedCompressedData)?;
    if reader.info().width != w || reader.info().height != h {
        return Err(RasterError::InvalidContainer);
    }
    let out_len = reader
        .output_buffer_size()
        .ok_or(RasterError::DecodedByteLimit)?;
    if out_len > limits.max_decoded_bytes {
        return Err(RasterError::DecodedByteLimit);
    }
    let mut raw = vec![0u8; out_len];
    let frame = reader
        .next_frame(&mut raw)
        .map_err(|_| RasterError::MalformedCompressedData)?;
    if frame.width != w || frame.height != h || frame.bit_depth != png::BitDepth::Eight {
        return Err(RasterError::MalformedCompressedData);
    }
    // Verify remaining chunks / terminal IEND cleanly.
    reader
        .finish()
        .map_err(|_| RasterError::MalformedCompressedData)?;

    let samples = &raw[..frame.buffer_size()];
    let pixel_count = (w as usize)
        .checked_mul(h as usize)
        .ok_or(RasterError::PixelCountLimit)?;
    let rgba8 = match frame.color_type {
        png::ColorType::Rgba => {
            if samples.len() != rgba_bytes {
                return Err(RasterError::MalformedCompressedData);
            }
            samples.to_vec()
        }
        png::ColorType::Rgb => {
            if samples.len() != pixel_count * 3 {
                return Err(RasterError::MalformedCompressedData);
            }
            let mut out = Vec::with_capacity(rgba_bytes);
            for chunk in samples.as_chunks::<3>().0 {
                out.extend_from_slice(&[chunk[0], chunk[1], chunk[2], 255]);
            }
            out
        }
        png::ColorType::GrayscaleAlpha => {
            if samples.len() != pixel_count * 2 {
                return Err(RasterError::MalformedCompressedData);
            }
            let mut out = Vec::with_capacity(rgba_bytes);
            for chunk in samples.as_chunks::<2>().0 {
                out.extend_from_slice(&[chunk[0], chunk[0], chunk[0], chunk[1]]);
            }
            out
        }
        png::ColorType::Grayscale => {
            if samples.len() != pixel_count {
                return Err(RasterError::MalformedCompressedData);
            }
            let mut out = Vec::with_capacity(rgba_bytes);
            for &g in samples {
                out.extend_from_slice(&[g, g, g, 255]);
            }
            out
        }
        png::ColorType::Indexed => return Err(RasterError::MalformedCompressedData),
    };

    Ok(DecodedRaster {
        format: RasterFormat::Png,
        width: w,
        height: h,
        rgba8,
    })
}

fn decode_jpeg(
    bytes: &[u8],
    expected: Option<(u32, u32)>,
    limits: &RasterLimits,
) -> Result<DecodedRaster, RasterError> {
    let (w, h) = jpeg_dimensions(bytes).ok_or(RasterError::InvalidContainer)?;
    let rgba_bytes = check_geometry(w, h, expected, limits)?;

    let mut decoder = jpeg_decoder::Decoder::new(Cursor::new(bytes));
    decoder
        .read_info()
        .map_err(|_| RasterError::MalformedCompressedData)?;
    let info = decoder.info().ok_or(RasterError::MalformedCompressedData)?;
    if u32::from(info.width) != w || u32::from(info.height) != h {
        return Err(RasterError::InvalidContainer);
    }
    let samples = decoder
        .decode()
        .map_err(|_| RasterError::MalformedCompressedData)?;
    let pixel_count = (w as usize)
        .checked_mul(h as usize)
        .ok_or(RasterError::PixelCountLimit)?;
    let rgba8 = match info.pixel_format {
        jpeg_decoder::PixelFormat::RGB24 => {
            if samples.len() != pixel_count * 3 {
                return Err(RasterError::MalformedCompressedData);
            }
            let mut out = Vec::with_capacity(rgba_bytes);
            for chunk in samples.as_chunks::<3>().0 {
                out.extend_from_slice(&[chunk[0], chunk[1], chunk[2], 255]);
            }
            out
        }
        jpeg_decoder::PixelFormat::L8 => {
            if samples.len() != pixel_count {
                return Err(RasterError::MalformedCompressedData);
            }
            let mut out = Vec::with_capacity(rgba_bytes);
            for &g in &samples {
                out.extend_from_slice(&[g, g, g, 255]);
            }
            out
        }
        _ => return Err(RasterError::InvalidContainer),
    };

    Ok(DecodedRaster {
        format: RasterFormat::Jpeg,
        width: w,
        height: h,
        rgba8,
    })
}

/// Fully decode and validate an imported PNG or JPEG using default B5-07 limits.
pub fn decode_bounded_raster(media_type: &str, bytes: &[u8]) -> Result<DecodedRaster, RasterError> {
    decode_bounded_raster_checked(media_type, bytes, None, &RasterLimits::DEFAULT)
}

/// Fully decode and validate an imported PNG or JPEG against explicit bounds
/// and optional declared `(width, height)` metadata.
pub fn decode_bounded_raster_checked(
    media_type: &str,
    bytes: &[u8],
    expected_dimensions: Option<(u32, u32)>,
    limits: &RasterLimits,
) -> Result<DecodedRaster, RasterError> {
    let format =
        RasterFormat::from_media_type(media_type).ok_or(RasterError::UnsupportedMediaType)?;
    if bytes.is_empty() || bytes.len() > limits.max_input_bytes {
        return Err(if bytes.is_empty() {
            RasterError::InvalidContainer
        } else {
            RasterError::InputSizeLimit
        });
    }
    match format {
        RasterFormat::Png => decode_png(bytes, expected_dimensions, limits),
        RasterFormat::Jpeg => decode_jpeg(bytes, expected_dimensions, limits),
    }
}
