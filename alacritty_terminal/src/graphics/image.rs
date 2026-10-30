use std::io::Read;

use flate2::read::ZlibDecoder;

use crate::graphics::command::{Compression, Format};

const MAX_IMAGE_DIMENSION: u32 = 10_000;
const MAX_DECOMPRESSED_SIZE: usize = 400_000_000;
const MAX_LOADING_SIZE: usize = if cfg!(test) { 8192 } else { 400_000_000 };

/// A decoded image ready for rendering.
#[derive(Debug, Clone)]
pub struct Image {
    pub id: u32,
    pub width: u32,
    pub height: u32,
    /// Always RGBA (4 bytes per pixel), decoded from original format.
    pub data: Vec<u8>,
    /// Image number (`I=` transmission parameter), if any.
    pub image_number: u32,
    /// Whether this image has an implicitly assigned ID.
    pub implicit_id: bool,
}

/// A partially-received image (chunked transmission).
#[derive(Debug)]
pub struct LoadingImage {
    pub id: u32,
    pub width: u32,
    pub height: u32,
    pub format: Format,
    pub compression: Compression,
    pub implicit_id: bool,
    pub image_number: u32,
    data: Vec<u8>,

    /// Saved display intent from the first chunk of a multi-chunk transmit.
    /// When `a=T` starts a chunked upload (`m=1`), we record the cursor
    /// position and display parameters so the placement can be created when
    /// the final chunk completes.
    pub pending_display: Option<PendingDisplay>,
}

/// Saved display parameters from the initial chunk of a multi-part upload.
#[derive(Debug, Clone)]
pub struct PendingDisplay {
    pub cursor_row: i32,
    pub cursor_col: usize,
    pub display: super::command::DisplayCmd,
    pub placement_id: u32,
    pub quiet: super::command::Quiet,
}

impl LoadingImage {
    pub fn new(
        id: u32,
        width: u32,
        height: u32,
        format: Format,
        compression: Compression,
        implicit_id: bool,
    ) -> Self {
        Self::with_number(id, width, height, format, compression, implicit_id, 0)
    }

    pub fn with_number(
        id: u32,
        width: u32,
        height: u32,
        format: Format,
        compression: Compression,
        implicit_id: bool,
        image_number: u32,
    ) -> Self {
        Self {
            id,
            width,
            height,
            format,
            compression,
            implicit_id,
            image_number,
            data: Vec::new(),
            pending_display: None,
        }
    }

    /// Append more data to the loading image.
    pub fn add_data(&mut self, data: &[u8]) -> Result<(), ImageError> {
        if self.data.len() + data.len() > MAX_LOADING_SIZE {
            return Err(ImageError::DataTooLarge);
        }
        self.data.extend_from_slice(data);
        Ok(())
    }

    /// Complete the image: decompress + decode → Image.
    pub fn complete(self) -> Result<Image, ImageError> {
        if self.data.is_empty() {
            return Err(ImageError::EmptyData);
        }

        if self.width > MAX_IMAGE_DIMENSION || self.height > MAX_IMAGE_DIMENSION {
            return Err(ImageError::DimensionMismatch {
                expected: MAX_IMAGE_DIMENSION as usize,
                actual: self.width.max(self.height) as usize,
            });
        }

        let max_decompressed = decompression_limit(self.width, self.height, self.format);
        let raw = decompress(self.compression, &self.data, max_decompressed)?;
        let (width, height, rgba) = decode_to_rgba(self.format, self.width, self.height, &raw)?;

        Ok(Image {
            id: self.id,
            width,
            height,
            data: rgba,
            image_number: self.image_number,
            implicit_id: self.implicit_id,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageError {
    /// Decompression failed.
    DecompressError(String),
    /// Image dimensions don't match data size.
    DimensionMismatch { expected: usize, actual: usize },
    /// PNG decode failed.
    PngDecodeError(String),
    /// Image is empty.
    EmptyData,
    /// Accumulated transmission data exceeds the maximum size.
    DataTooLarge,
}

fn decompression_limit(width: u32, height: u32, format: Format) -> usize {
    if width > 0 && height > 0 && format != Format::Png {
        let bytes_per_pixel = if format == Format::Rgb { 3 } else { 4 };
        let pixel_bytes = (width as usize)
            .saturating_mul(height as usize)
            .saturating_mul(bytes_per_pixel);
        return MAX_DECOMPRESSED_SIZE.min(pixel_bytes);
    }
    MAX_DECOMPRESSED_SIZE
}

fn decompress_zlib(data: &[u8], max_size: usize) -> Result<Vec<u8>, ImageError> {
    let decoder = ZlibDecoder::new(data);
    let mut decompressed = Vec::new();
    decoder
        .take((max_size + 1) as u64)
        .read_to_end(&mut decompressed)
        .map_err(|err| ImageError::DecompressError(err.to_string()))?;
    if decompressed.len() > max_size {
        return Err(ImageError::DecompressError(
            "decompressed data exceeds maximum size".into(),
        ));
    }
    Ok(decompressed)
}

fn decompress(compression: Compression, data: &[u8], max_size: usize) -> Result<Vec<u8>, ImageError> {
    match compression {
        Compression::None => {
            if data.len() > max_size {
                return Err(ImageError::DecompressError(
                    "decompressed data exceeds maximum size".into(),
                ));
            }
            Ok(data.to_vec())
        },
        Compression::Zlib => decompress_zlib(data, max_size),
    }
}

fn decode_to_rgba(
    format: Format,
    width: u32,
    height: u32,
    data: &[u8],
) -> Result<(u32, u32, Vec<u8>), ImageError> {
    match format {
        Format::Rgba => {
            let (width, height) = resolve_dimensions(width, height, data.len(), 4)?;
            validate_dimensions(width, height, data.len())?;
            Ok((width, height, data.to_vec()))
        },
        Format::Rgb => {
            if data.len() % 3 != 0 {
                return Err(ImageError::DimensionMismatch {
                    expected: data.len() - data.len() % 3,
                    actual: data.len(),
                });
            }
            let pixel_count = data.len() / 3;
            let (width, height) = resolve_dimensions(width, height, pixel_count * 4, 4)?;
            let expected_pixels = (width as usize).saturating_mul(height as usize);
            if expected_pixels != pixel_count {
                return Err(ImageError::DimensionMismatch {
                    expected: expected_pixels * 3,
                    actual: data.len(),
                });
            }

            let mut rgba = Vec::with_capacity(pixel_count * 4);
            for chunk in data.chunks_exact(3) {
                rgba.extend_from_slice(chunk);
                rgba.push(255);
            }
            Ok((width, height, rgba))
        },
        Format::Png => decode_png(data),
    }
}

fn decode_png(data: &[u8]) -> Result<(u32, u32, Vec<u8>), ImageError> {
    use std::io::Cursor;

    let mut decoder = png::Decoder::new(Cursor::new(data));
    // Normalize palette/low-bit-depth/16-bit inputs to 8-bit samples so the
    // color-type match below covers every PNG a client can legally send.
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder
        .read_info()
        .map_err(|err| ImageError::PngDecodeError(err.to_string()))?;

    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut buf)
        .map_err(|err| ImageError::PngDecodeError(err.to_string()))?;

    buf.truncate(info.buffer_size());

    let width = info.width;
    let height = info.height;
    if width > MAX_IMAGE_DIMENSION || height > MAX_IMAGE_DIMENSION {
        return Err(ImageError::DimensionMismatch {
            expected: MAX_IMAGE_DIMENSION as usize,
            actual: width.max(height) as usize,
        });
    }
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => {
            let mut rgba = Vec::with_capacity(buf.len() / 3 * 4);
            for chunk in buf.chunks_exact(3) {
                rgba.extend_from_slice(chunk);
                rgba.push(255);
            }
            rgba
        },
        png::ColorType::Grayscale => {
            let mut rgba = Vec::with_capacity(buf.len() * 4);
            for &gray in &buf {
                rgba.extend_from_slice(&[gray, gray, gray, 255]);
            }
            rgba
        },
        png::ColorType::GrayscaleAlpha => {
            let mut rgba = Vec::with_capacity(buf.len() / 2 * 4);
            for chunk in buf.chunks_exact(2) {
                rgba.extend_from_slice(&[chunk[0], chunk[0], chunk[0], chunk[1]]);
            }
            rgba
        },
        _ => return Err(ImageError::PngDecodeError("unsupported PNG color type".into())),
    };

    validate_dimensions(width, height, rgba.len())?;
    Ok((width, height, rgba))
}

fn resolve_dimensions(
    width: u32,
    height: u32,
    rgba_len: usize,
    bytes_per_pixel: usize,
) -> Result<(u32, u32), ImageError> {
    if rgba_len % bytes_per_pixel != 0 {
        return Err(ImageError::DimensionMismatch { expected: rgba_len, actual: rgba_len });
    }
    let pixels = rgba_len / bytes_per_pixel;

    match (width, height) {
        (0, 0) => Err(ImageError::DimensionMismatch { expected: 0, actual: rgba_len }),
        (0, h) if h > 0 => {
            if pixels % h as usize != 0 {
                return Err(ImageError::DimensionMismatch { expected: pixels * bytes_per_pixel, actual: rgba_len });
            }
            Ok((pixels as u32 / h, h))
        },
        (w, 0) if w > 0 => {
            if pixels % w as usize != 0 {
                return Err(ImageError::DimensionMismatch { expected: pixels * bytes_per_pixel, actual: rgba_len });
            }
            Ok((w, pixels as u32 / w))
        },
        (w, h) => Ok((w, h)),
    }
}

fn validate_dimensions(width: u32, height: u32, len: usize) -> Result<(), ImageError> {
    let expected = (width as usize).saturating_mul(height as usize).saturating_mul(4);
    if expected != len {
        return Err(ImageError::DimensionMismatch { expected, actual: len });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::ZlibEncoder;
    use std::io::Write;

    fn zlib_compress(data: &[u8]) -> Vec<u8> {
        let mut encoder = ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(data).unwrap();
        encoder.finish().unwrap()
    }

    fn make_png_1x1_red() -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut buf, 1, 1);
            encoder.set_color(png::ColorType::Rgb);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&[255, 0, 0]).unwrap();
        }
        buf
    }

    #[test]
    fn test_decode_rgba() {
        let data: Vec<u8> = vec![255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255];
        let loading = LoadingImage::new(1, 2, 2, Format::Rgba, Compression::None, false);
        let mut loading = loading;
        loading.add_data(&data).unwrap();
        let image = loading.complete().unwrap();
        assert_eq!(image.width, 2);
        assert_eq!(image.height, 2);
        assert_eq!(image.data.len(), 16);
    }

    #[test]
    fn test_decode_rgb_to_rgba() {
        let data: Vec<u8> = vec![255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255];
        let mut loading = LoadingImage::new(1, 2, 2, Format::Rgb, Compression::None, false);
        loading.add_data(&data).unwrap();
        let image = loading.complete().unwrap();
        assert_eq!(image.data.len(), 16);
        assert_eq!(&image.data[0..4], &[255, 0, 0, 255]);
    }

    #[test]
    fn test_decode_rgb_dimension_mismatch() {
        let data: Vec<u8> = vec![255, 0, 0, 0, 255, 0];
        let mut loading = LoadingImage::new(1, 2, 2, Format::Rgb, Compression::None, false);
        loading.add_data(&data).unwrap();
        assert!(matches!(
            loading.complete(),
            Err(ImageError::DimensionMismatch { expected: 12, actual: 6 })
        ));
    }

    #[test]
    fn test_decode_zlib_rgba() {
        let raw: Vec<u8> = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
        let compressed = zlib_compress(&raw);
        let mut loading = LoadingImage::new(1, 2, 2, Format::Rgba, Compression::Zlib, false);
        loading.add_data(&compressed).unwrap();
        let image = loading.complete().unwrap();
        assert_eq!(image.data, raw);
    }

    #[test]
    fn test_decode_zlib_rgb() {
        let raw: Vec<u8> = vec![10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120];
        let compressed = zlib_compress(&raw);
        let mut loading = LoadingImage::new(1, 2, 2, Format::Rgb, Compression::Zlib, false);
        loading.add_data(&compressed).unwrap();
        let image = loading.complete().unwrap();
        assert_eq!(image.data.len(), 16);
        assert_eq!(image.data[3], 255);
    }

    #[test]
    fn test_decode_empty_data() {
        let loading = LoadingImage::new(1, 1, 1, Format::Rgba, Compression::None, false);
        assert!(matches!(loading.complete(), Err(ImageError::EmptyData)));
    }

    #[test]
    fn test_chunked_loading() {
        let mut loading = LoadingImage::new(1, 2, 2, Format::Rgba, Compression::None, false);
        loading.add_data(&[1, 2, 3, 4, 5, 6, 7, 8]).unwrap();
        loading.add_data(&[9, 10, 11, 12, 13, 14, 15, 16]).unwrap();
        let image = loading.complete().unwrap();
        assert_eq!(image.data.len(), 16);
    }

    #[test]
    fn test_decode_png() {
        let png = make_png_1x1_red();
        let mut loading = LoadingImage::new(1, 0, 0, Format::Png, Compression::None, false);
        loading.add_data(&png).unwrap();
        let image = loading.complete().unwrap();
        assert_eq!(image.width, 1);
        assert_eq!(image.height, 1);
        assert_eq!(&image.data[0..4], &[255, 0, 0, 255]);
    }

    #[test]
    fn test_decode_corrupt_zlib() {
        let mut loading = LoadingImage::new(1, 1, 1, Format::Rgba, Compression::Zlib, false);
        loading.add_data(b"not valid zlib").unwrap();
        assert!(matches!(
            loading.complete(),
            Err(ImageError::DecompressError(_))
        ));
    }

    #[test]
    fn test_decode_corrupt_png() {
        let mut loading = LoadingImage::new(1, 0, 0, Format::Png, Compression::None, false);
        loading.add_data(b"not a png").unwrap();
        assert!(matches!(loading.complete(), Err(ImageError::PngDecodeError(_))));
    }

    fn make_png(color: png::ColorType, depth: png::BitDepth, palette: &[u8], data: &[u8]) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut buf, 1, 1);
            encoder.set_color(color);
            encoder.set_depth(depth);
            if !palette.is_empty() {
                encoder.set_palette(palette.to_vec());
            }
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(data).unwrap();
        }
        buf
    }

    #[test]
    fn test_decode_png_indexed() {
        let png = make_png(png::ColorType::Indexed, png::BitDepth::Eight, &[255, 0, 0], &[0]);
        let mut loading = LoadingImage::new(1, 0, 0, Format::Png, Compression::None, false);
        loading.add_data(&png).unwrap();
        let image = loading.complete().unwrap();
        assert_eq!(&image.data[0..4], &[255, 0, 0, 255]);
    }

    #[test]
    fn test_decode_png_16bit() {
        // One RGB pixel, R=0xFF00 (big-endian 16-bit), G=B=0.
        let png = make_png(png::ColorType::Rgb, png::BitDepth::Sixteen, &[], &[255, 0, 0, 0, 0, 0]);
        let mut loading = LoadingImage::new(1, 0, 0, Format::Png, Compression::None, false);
        loading.add_data(&png).unwrap();
        let image = loading.complete().unwrap();
        assert_eq!(&image.data[0..4], &[255, 0, 0, 255]);
    }

    #[test]
    fn test_decode_max_dimension_exceeded() {
        let mut loading = LoadingImage::new(1, 20_000, 1, Format::Rgba, Compression::None, false);
        loading.add_data(&[0; 4]).unwrap();
        assert!(matches!(
            loading.complete(),
            Err(ImageError::DimensionMismatch { expected: 10_000, actual: 20_000 })
        ));
    }

    #[test]
    fn test_decode_loading_data_too_large() {
        let mut loading = LoadingImage::new(1, 1, 1, Format::Rgba, Compression::None, false);
        loading.add_data(&vec![0u8; MAX_LOADING_SIZE]).unwrap();
        assert_eq!(loading.add_data(&[0u8; 1]), Err(ImageError::DataTooLarge));
    }
}
