//! Original-resolution PNG assembly without a full-image framebuffer.
//! Tiles arrive in row-major order; only one horizontal band is retained.
use base64::Engine;
use serde::Serialize;
use std::{
    collections::HashMap,
    fs::File,
    io::{Cursor, Read, Seek, SeekFrom, Write},
    path::PathBuf,
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

const TILE_WIDTH: u32 = 2048;
const MAX_BAND_HEIGHT: u32 = 512;
const BAND_BYTES: usize = 32 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
pub struct ExportInfo {
    pub id: u64,
    pub width: u32,
    pub height: u32,
    pub tile_width: u32,
    pub tile_height: u32,
}

#[derive(Default)]
pub struct NetworkPngExports {
    next: AtomicU64,
    sessions: Mutex<HashMap<u64, PngExport>>,
}

impl NetworkPngExports {
    pub fn begin(&self, path: PathBuf, width: u32, height: u32) -> Result<ExportInfo, String> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let session = PngExport::new(path, width, height)?;
        let info = ExportInfo {
            id,
            width,
            height,
            tile_width: TILE_WIDTH,
            tile_height: session.band_height,
        };
        self.sessions
            .lock()
            .map_err(|e| e.to_string())?
            .insert(id, session);
        Ok(info)
    }
    pub fn tile(&self, id: u64, x: u32, y: u32, encoded: &str) -> Result<(), String> {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|e| e.to_string())?;
        let mut sessions = self.sessions.lock().map_err(|e| e.to_string())?;
        sessions
            .get_mut(&id)
            .ok_or("PNG 导出会话已结束")?
            .tile(x, y, &bytes)
    }
    pub fn finish(&self, id: u64) -> Result<String, String> {
        let session = self
            .sessions
            .lock()
            .map_err(|e| e.to_string())?
            .remove(&id)
            .ok_or("PNG 导出会话已结束")?;
        session.finish()
    }
    pub fn abort(&self, id: u64) -> Result<(), String> {
        self.sessions.lock().map_err(|e| e.to_string())?.remove(&id);
        Ok(())
    }
}

struct PngExport {
    // Drop the writer before removing its temporary file on Windows.
    stream: Option<png::StreamWriter<'static, File>>,
    temp: tempfile::NamedTempFile,
    destination: PathBuf,
    width: u32,
    height: u32,
    band_height: u32,
    x: u32,
    y: u32,
    band: Vec<u8>,
}

impl PngExport {
    fn new(destination: PathBuf, width: u32, height: u32) -> Result<Self, String> {
        if width == 0 || height == 0 {
            return Err("PNG 尺寸必须大于零".into());
        }
        let row_bytes = (width as usize).checked_mul(4).ok_or("PNG 行宽溢出")?;
        // This bounds working memory, not output dimensions or resolution.
        let band_height = MAX_BAND_HEIGHT
            .min((BAND_BYTES / row_bytes).max(1) as u32)
            .min(height);
        let bytes = row_bytes
            .checked_mul(band_height as usize)
            .ok_or("PNG 条带大小溢出")?;
        let mut band = Vec::new();
        band.try_reserve_exact(bytes)
            .map_err(|e| format!("无法分配 PNG 条带：{e}"))?;
        band.resize(bytes, 0);
        let temp = tempfile::NamedTempFile::new_in(
            destination.parent().unwrap_or(std::path::Path::new(".")),
        )
        .map_err(|e| e.to_string())?;
        let mut encoder =
            png::Encoder::new(temp.reopen().map_err(|e| e.to_string())?, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Fast);
        let stream = encoder
            .write_header()
            .map_err(|e| e.to_string())?
            .into_stream_writer_with_size(64 * 1024)
            .map_err(|e| e.to_string())?;
        Ok(Self {
            stream: Some(stream),
            temp,
            destination,
            width,
            height,
            band_height,
            x: 0,
            y: 0,
            band,
        })
    }
    fn tile(&mut self, x: u32, y: u32, bytes: &[u8]) -> Result<(), String> {
        if self.y >= self.height || x != self.x || y != self.y {
            return Err("PNG 分区顺序或坐标不正确".into());
        }
        let width = TILE_WIDTH.min(self.width - x);
        let height = self.band_height.min(self.height - y);
        let mut decoder = png::Decoder::new(Cursor::new(bytes));
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
        if reader.info().width != width || reader.info().height != height {
            return Err("PNG 分区尺寸不匹配".into());
        }
        let mut pixels = vec![0; reader.output_buffer_size().ok_or("PNG 分区大小溢出")?];
        let info = reader.next_frame(&mut pixels).map_err(|e| e.to_string())?;
        let channels = match info.color_type {
            png::ColorType::Rgba => 4,
            png::ColorType::Rgb => 3,
            png::ColorType::GrayscaleAlpha => 2,
            png::ColorType::Grayscale => 1,
            _ => return Err("不支持的 PNG 分区颜色类型".into()),
        };
        for row in 0..height as usize {
            if channels == 4 {
                let src = row * width as usize * 4;
                let dst = (row * self.width as usize + x as usize) * 4;
                self.band[dst..dst + width as usize * 4]
                    .copy_from_slice(&pixels[src..src + width as usize * 4]);
                continue;
            }
            for col in 0..width as usize {
                let src = (row * width as usize + col) * channels;
                let dst = (row * self.width as usize + x as usize + col) * 4;
                let rgba = match channels {
                    4 => [
                        pixels[src],
                        pixels[src + 1],
                        pixels[src + 2],
                        pixels[src + 3],
                    ],
                    3 => [pixels[src], pixels[src + 1], pixels[src + 2], 255],
                    2 => [pixels[src], pixels[src], pixels[src], pixels[src + 1]],
                    _ => [pixels[src], pixels[src], pixels[src], 255],
                };
                self.band[dst..dst + 4].copy_from_slice(&rgba);
            }
        }
        self.x += width;
        if self.x == self.width {
            let bytes = self.width as usize * height as usize * 4;
            self.stream
                .as_mut()
                .ok_or("PNG 编码器已结束")?
                .write_all(&self.band[..bytes])
                .map_err(|e| e.to_string())?;
            self.x = 0;
            self.y += height;
        }
        Ok(())
    }
    fn finish(mut self) -> Result<String, String> {
        if self.y != self.height || self.x != 0 {
            return Err("PNG 分区尚未全部接收，未保存文件".into());
        }
        self.stream
            .take()
            .ok_or("PNG 编码器已结束")?
            .finish()
            .map_err(|e| e.to_string())?;
        // png's owned Writer writes IEND on drop; detect a failed final write
        // before replacing an existing destination.
        let file = self.temp.as_file_mut();
        file.seek(SeekFrom::End(-12)).map_err(|e| e.to_string())?;
        let mut end = [0; 12];
        file.read_exact(&mut end).map_err(|e| e.to_string())?;
        if end != [0, 0, 0, 0, b'I', b'E', b'N', b'D', 174, 66, 96, 130] {
            return Err("PNG 编码未完整结束".into());
        }
        file.sync_all().map_err(|e| e.to_string())?;
        self.temp
            .persist(&self.destination)
            .map_err(|e| e.to_string())?;
        Ok(self.destination.to_string_lossy().into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pixel(x: u32, y: u32) -> [u8; 4] {
        [
            (x % 251) as u8,
            (y % 239) as u8,
            ((x + y) % 253) as u8,
            128 + (x % 128) as u8,
        ]
    }
    fn tile_png(x: u32, y: u32, w: u32, h: u32) -> Vec<u8> {
        let pixels: Vec<u8> = (0..h)
            .flat_map(|dy| (0..w).flat_map(move |dx| pixel(x + dx, y + dy)))
            .collect();
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, w, h);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&pixels).unwrap();
            writer.finish().unwrap();
        }
        bytes
    }
    #[test]
    fn assembles_full_resolution_pixels_across_horizontal_and_vertical_seams() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("result.png");
        let exports = NetworkPngExports::default();
        let info = exports.begin(path.clone(), 2053, 515).unwrap();
        for y in (0..info.height).step_by(info.tile_height as usize) {
            for x in (0..info.width).step_by(info.tile_width as usize) {
                let bytes = tile_png(
                    x,
                    y,
                    info.tile_width.min(info.width - x),
                    info.tile_height.min(info.height - y),
                );
                exports
                    .tile(
                        info.id,
                        x,
                        y,
                        &base64::engine::general_purpose::STANDARD.encode(bytes),
                    )
                    .unwrap();
            }
        }
        assert!(
            !path.exists(),
            "destination only appears after successful finish"
        );
        assert_eq!(exports.finish(info.id).unwrap(), path.to_string_lossy());
        let mut decoder = png::Decoder::new(std::io::BufReader::new(File::open(path).unwrap()))
            .read_info()
            .unwrap();
        let mut output = vec![0; decoder.output_buffer_size().unwrap()];
        let frame = decoder.next_frame(&mut output).unwrap();
        assert_eq!((frame.width, frame.height), (2053, 515));
        for y in 0..515 {
            for x in 0..2053 {
                let offset = (y as usize * 2053 + x as usize) * 4;
                assert_eq!(&output[offset..offset + 4], &pixel(x, y));
            }
        }
    }
    #[test]
    fn rejects_invalid_order_dimensions_and_payload_without_advancing() {
        let dir = tempfile::tempdir().unwrap();
        let exports = NetworkPngExports::default();
        let info = exports.begin(dir.path().join("x.png"), 2, 1).unwrap();
        let encoded = base64::engine::general_purpose::STANDARD.encode(tile_png(0, 0, 2, 1));
        assert!(exports.tile(info.id, 1, 0, &encoded).is_err());
        assert!(exports.tile(info.id, 0, 0, "bad base64").is_err());
        assert!(
            exports
                .tile(
                    info.id,
                    0,
                    0,
                    &base64::engine::general_purpose::STANDARD.encode(tile_png(0, 0, 1, 1))
                )
                .is_err()
        );
        exports.tile(info.id, 0, 0, &encoded).unwrap();
        assert!(exports.tile(info.id, 0, 0, &encoded).is_err());
        exports.finish(info.id).unwrap();
    }
    #[test]
    fn incomplete_export_and_cancellation_preserve_existing_destination() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("existing.png");
        std::fs::write(&path, b"original").unwrap();
        let exports = NetworkPngExports::default();
        let info = exports.begin(path.clone(), 2, 2).unwrap();
        assert!(exports.finish(info.id).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"original");
        let info = exports.begin(path.clone(), 25288, 7092).unwrap();
        assert!(exports.sessions.lock().unwrap()[&info.id].band.len() <= BAND_BYTES);
        exports.abort(info.id).unwrap();
        assert!(exports.finish(info.id).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"original");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }
    #[test]
    fn finished_export_atomically_replaces_existing_destination() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("existing.png");
        std::fs::write(&path, b"original").unwrap();
        let exports = NetworkPngExports::default();
        let info = exports.begin(path.clone(), 1, 1).unwrap();
        exports
            .tile(
                info.id,
                0,
                0,
                &base64::engine::general_purpose::STANDARD.encode(tile_png(0, 0, 1, 1)),
            )
            .unwrap();
        exports.finish(info.id).unwrap();
        assert_eq!(&std::fs::read(path).unwrap()[..8], b"\x89PNG\r\n\x1a\n");
    }
}
