//! B5-07 bounded full pixel decoding tests for imported PNG and JPEG rasters.

use base64::{engine::general_purpose::STANDARD, Engine as _};
use hmi_legacy_import::raster::{
    decode_bounded_raster, decode_bounded_raster_checked, RasterError, RasterFormat, RasterLimits,
};

// Original synthetic 1x1 white opaque PNG (grayscale+alpha).
const VALID_PNG: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 4, 0,
    0, 0, 181, 28, 12, 2, 0, 0, 0, 11, 73, 68, 65, 84, 120, 218, 99, 252, 255, 31, 0, 3, 3, 2, 0,
    239, 162, 167, 91, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

// Original synthetic 2x3 white baseline sequential JPEG.
const VALID_JPEG_B64: &str = "/9j/4AAQSkZJRgABAQAAAQABAAD/2wBDAAgGBgcGBQgHBwcJCQgKDBQNDAsLDBkSEw8UHRofHh0aHBwgJC4nICIsIxwcKDcpLDAxNDQ0Hyc5PTgyPC4zNDL/2wBDAQkJCQwLDBgNDRgyIRwhMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjL/wAARCAADAAIDASIAAhEBAxEB/8QAHwAAAQUBAQEBAQEAAAAAAAAAAAECAwQFBgcICQoL/8QAtRAAAgEDAwIEAwUFBAQAAAF9AQIDAAQRBRIhMUEGE1FhByJxFDKBkaEII0KxwRVS0fAkM2JyggkKFhcYGRolJicoKSo0NTY3ODk6Q0RFRkdISUpTVFVWV1hZWmNkZWZnaGlqc3R1dnd4eXqDhIWGh4iJipKTlJWWl5iZmqKjpKWmp6ipqrKztLW2t7i5usLDxMXGx8jJytLT1NXW19jZ2uHi4+Tl5ufo6erx8vP09fb3+Pn6/8QAHwEAAwEBAQEBAQEBAQAAAAAAAAECAwQFBgcICQoL/8QAtREAAgECBAQDBAcFBAQAAQJ3AAECAxEEBSExBhJBUQdhcRMiMoEIFEKRobHBCSMzUvAVYnLRChYkNOEl8RcYGRomJygpKjU2Nzg5OkNERUZHSElKU1RVVldYWVpjZGVmZ2hpanN0dXZ3eHl6goOEhYaHiImKkpOUlZaXmJmaoqOkpaanqKmqsrO0tba3uLm6wsPExcbHyMnK0tPU1dbX2Nna4uPk5ebn6Onq8vP09fb3+Pn6/9oADAMBAAIRAxEAPwD3+iiigD//2Q==";

fn png_crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb88320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}

fn png_chunk(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(12 + payload.len());
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(payload);
    let crc = png_crc32(&out[4..]);
    out.extend_from_slice(&crc.to_be_bytes());
    out
}

fn synthetic_png_envelope(width: u32, height: u32, idat_payload: &[u8]) -> Vec<u8> {
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8-bit RGBA
    out.extend_from_slice(&png_chunk(b"IHDR", &ihdr));
    out.extend_from_slice(&png_chunk(b"IDAT", idat_payload));
    out.extend_from_slice(&png_chunk(b"IEND", &[]));
    out
}

fn valid_rgb_png_2x2() -> Vec<u8> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, 2, 2);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer
        .write_image_data(&[255, 0, 0, 0, 255, 0, 0, 0, 255, 16, 32, 48])
        .unwrap();
    drop(writer);
    out
}

#[test]
fn valid_png_and_jpeg_decode_to_exact_rgba8_pixels() {
    let png = decode_bounded_raster("image/png", VALID_PNG).unwrap();
    assert_eq!(png.format(), RasterFormat::Png);
    assert_eq!((png.width(), png.height()), (1, 1));
    assert_eq!(png.rgba8(), &[255, 255, 255, 255]);
    assert!(format!("{png:?}").contains("rgba8_len: 4"));

    let rgb = decode_bounded_raster("image/png", &valid_rgb_png_2x2()).unwrap();
    assert_eq!((rgb.width(), rgb.height()), (2, 2));
    assert_eq!(
        rgb.rgba8(),
        &[255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 16, 32, 48, 255]
    );

    let jpeg_bytes = STANDARD.decode(VALID_JPEG_B64).unwrap();
    let jpeg = decode_bounded_raster("image/jpeg", &jpeg_bytes).unwrap();
    assert_eq!(jpeg.format(), RasterFormat::Jpeg);
    assert_eq!((jpeg.width(), jpeg.height()), (2, 3));
    assert_eq!(jpeg.rgba8().len(), 2 * 3 * 4);
    assert!(jpeg
        .rgba8()
        .as_chunks::<4>()
        .0
        .iter()
        .all(|px| px[3] == 255 && px[0] >= 250));
}

#[test]
fn dimension_pixel_decoded_byte_and_input_limits_fail_closed() {
    let png_2x2 = valid_rgb_png_2x2();
    let jpeg_2x3 = STANDARD.decode(VALID_JPEG_B64).unwrap();

    // Dimension limit
    let dim_limit = RasterLimits {
        max_width: 1,
        max_height: 1,
        ..RasterLimits::DEFAULT
    };
    assert_eq!(
        decode_bounded_raster_checked("image/png", &png_2x2, None, &dim_limit),
        Err(RasterError::DimensionLimit)
    );
    assert_eq!(
        decode_bounded_raster_checked("image/jpeg", &jpeg_2x3, None, &dim_limit),
        Err(RasterError::DimensionLimit)
    );

    // Pixel-count limit
    let px_limit = RasterLimits {
        max_pixels: 3,
        ..RasterLimits::DEFAULT
    };
    assert_eq!(
        decode_bounded_raster_checked("image/png", &png_2x2, None, &px_limit),
        Err(RasterError::PixelCountLimit)
    );
    assert_eq!(
        decode_bounded_raster_checked("image/jpeg", &jpeg_2x3, None, &px_limit),
        Err(RasterError::PixelCountLimit)
    );

    // Decoded-byte limit (2x2 RGBA8 = 16 bytes; 2x3 RGBA8 = 24 bytes)
    let byte_limit = RasterLimits {
        max_decoded_bytes: 12,
        ..RasterLimits::DEFAULT
    };
    assert_eq!(
        decode_bounded_raster_checked("image/png", &png_2x2, None, &byte_limit),
        Err(RasterError::DecodedByteLimit)
    );
    assert_eq!(
        decode_bounded_raster_checked("image/jpeg", &jpeg_2x3, None, &byte_limit),
        Err(RasterError::DecodedByteLimit)
    );

    // Compressed input byte limit
    let input_limit = RasterLimits {
        max_input_bytes: 16,
        ..RasterLimits::DEFAULT
    };
    assert_eq!(
        decode_bounded_raster_checked("image/png", VALID_PNG, None, &input_limit),
        Err(RasterError::InputSizeLimit)
    );
    assert_eq!(
        decode_bounded_raster_checked("image/jpeg", &jpeg_2x3, None, &input_limit),
        Err(RasterError::InputSizeLimit)
    );

    // Declared dimension mismatch
    assert_eq!(
        decode_bounded_raster_checked("image/png", &png_2x2, Some((2, 3)), &RasterLimits::DEFAULT),
        Err(RasterError::DimensionMismatch)
    );
    assert_eq!(
        decode_bounded_raster_checked(
            "image/jpeg",
            &jpeg_2x3,
            Some((3, 2)),
            &RasterLimits::DEFAULT
        ),
        Err(RasterError::DimensionMismatch)
    );

    // Default 4096 dimension limit rejects 5000x1 structurally valid PNG envelope
    let oversized_png =
        synthetic_png_envelope(5000, 1, &[0x78, 0x9c, 0x03, 0x00, 0x00, 0x00, 0x00, 0x01]);
    assert_eq!(
        decode_bounded_raster("image/png", &oversized_png),
        Err(RasterError::DimensionLimit)
    );
}

#[test]
fn malformed_compressed_png_and_jpeg_streams_are_refused() {
    // Structurally valid PNG envelope (valid chunk CRCs and IEND) with corrupt zlib data.
    let corrupt_zlib_png = synthetic_png_envelope(2, 2, &[0xde, 0xad, 0xbe, 0xef]);
    assert_eq!(
        decode_bounded_raster("image/png", &corrupt_zlib_png),
        Err(RasterError::MalformedCompressedData)
    );

    // Structurally valid PNG with valid zlib header/empty deflate stream (truncated pixel rows).
    let truncated_rows_png =
        synthetic_png_envelope(2, 2, &[0x78, 0x9c, 0x03, 0x00, 0x00, 0x00, 0x00, 0x01]);
    assert_eq!(
        decode_bounded_raster("image/png", &truncated_rows_png),
        Err(RasterError::MalformedCompressedData)
    );

    // Structurally valid baseline JPEG envelope whose scan entropy payload contains
    // an illegal all-ones Huffman code (0xFF 0x00 byte-stuffed all-ones sequence).
    let valid_jpeg = STANDARD.decode(VALID_JPEG_B64).unwrap();
    let sos = valid_jpeg
        .windows(2)
        .position(|w| w == [0xff, 0xda])
        .unwrap();
    let sos_len = u16::from_be_bytes([valid_jpeg[sos + 2], valid_jpeg[sos + 3]]) as usize;
    let scan_start = sos + 2 + sos_len;
    let mut bad_huffman_jpeg = valid_jpeg[..scan_start].to_vec();
    bad_huffman_jpeg.extend_from_slice(&[0xff, 0x00, 0xff, 0x00, 0xff, 0x00, 0xff, 0xd9]);
    assert_eq!(
        decode_bounded_raster("image/jpeg", &bad_huffman_jpeg),
        Err(RasterError::MalformedCompressedData)
    );

    // Structurally valid baseline JPEG with a corrupted DHT segment (excessive symbol counts).
    let dht = valid_jpeg
        .windows(2)
        .position(|w| w == [0xff, 0xc4])
        .unwrap();
    let mut bad_dht_jpeg = valid_jpeg.clone();
    bad_dht_jpeg[dht + 5] = 0xff;
    assert_eq!(
        decode_bounded_raster("image/jpeg", &bad_dht_jpeg),
        Err(RasterError::MalformedCompressedData)
    );

    // Baseline JPEG with an illegal SOF0 marker embedded inside the scan stream.
    let mut bad_scan_marker_jpeg = valid_jpeg[..scan_start].to_vec();
    bad_scan_marker_jpeg.extend_from_slice(&[0x01, 0xff, 0xc0, 0x00, 0xff, 0xd9]);
    assert_eq!(
        decode_bounded_raster("image/jpeg", &bad_scan_marker_jpeg),
        Err(RasterError::InvalidContainer)
    );

    // Structurally valid baseline JPEG with an invalid DQT precision nibble (value 2).
    let dqt = valid_jpeg
        .windows(2)
        .position(|w| w == [0xff, 0xdb])
        .unwrap();
    let mut bad_dqt_jpeg = valid_jpeg.clone();
    bad_dqt_jpeg[dqt + 4] = 0x20;
    assert_eq!(
        decode_bounded_raster("image/jpeg", &bad_dqt_jpeg),
        Err(RasterError::MalformedCompressedData)
    );

    // Structurally valid baseline JPEG with the DHT table segment removed before SOS.
    let dht_len = u16::from_be_bytes([valid_jpeg[dht + 2], valid_jpeg[dht + 3]]) as usize;
    let mut missing_dht_jpeg = Vec::with_capacity(valid_jpeg.len());
    missing_dht_jpeg.extend_from_slice(&valid_jpeg[..dht]);
    missing_dht_jpeg.extend_from_slice(&valid_jpeg[dht + 2 + dht_len..]);
    assert_eq!(
        decode_bounded_raster("image/jpeg", &missing_dht_jpeg),
        Err(RasterError::MalformedCompressedData)
    );

    // Truncated JPEG missing EOI fails closed.
    assert_eq!(
        decode_bounded_raster("image/jpeg", &valid_jpeg[..valid_jpeg.len() - 2]),
        Err(RasterError::InvalidContainer)
    );

    // Trailing bytes after IEND / EOI and unsupported media types fail closed.
    let mut trailing_png = VALID_PNG.to_vec();
    trailing_png.push(0);
    assert_eq!(
        decode_bounded_raster("image/png", &trailing_png),
        Err(RasterError::InvalidContainer)
    );
    let mut trailing_jpeg = STANDARD.decode(VALID_JPEG_B64).unwrap();
    trailing_jpeg.push(0);
    assert_eq!(
        decode_bounded_raster("image/jpeg", &trailing_jpeg),
        Err(RasterError::InvalidContainer)
    );
    assert_eq!(
        decode_bounded_raster("image/svg+xml", b"<svg/>"),
        Err(RasterError::UnsupportedMediaType)
    );
    assert_eq!(
        decode_bounded_raster("image/png", &[]),
        Err(RasterError::InvalidContainer)
    );
}
