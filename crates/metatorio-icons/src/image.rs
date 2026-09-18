//! RGBA8 位图：PNG 解码/编码、直通↔预乘、重采样与合成。
//!
//! **alpha 约定**：本模块的缓冲一律是**直通 alpha**（straight，RGB 不预先乘 alpha），
//! 因为游戏的源 PNG 就是直通的；合成时内部转成预乘算，算完再转回去。

/// 直通 alpha 的 RGBA8 位图。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rgba8 {
    pub width: u32,
    pub height: u32,
    /// 行优先，长度 = width × height × 4。
    pub pixels: Vec<u8>,
}

impl Rgba8 {
    /// 全透明画布。
    pub fn transparent(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; (width as usize) * (height as usize) * 4],
        }
    }

    pub fn from_pixels(width: u32, height: u32, pixels: Vec<u8>) -> Self {
        assert_eq!(
            pixels.len(),
            (width as usize) * (height as usize) * 4,
            "像素数量与尺寸不匹配"
        );
        Self {
            width,
            height,
            pixels,
        }
    }

    /// 取左上角 `size × size` 的方块（Factorio 的图标文件是「mipmap 横排」，
    /// level 0 就是左上角那一块）。
    pub fn top_left_tile(&self, size: u32) -> Rgba8 {
        let size = size.min(self.width).min(self.height);
        let mut pixels = Vec::with_capacity((size as usize) * (size as usize) * 4);
        for y in 0..size {
            let start = ((y * self.width) * 4) as usize;
            pixels.extend_from_slice(&self.pixels[start..start + (size as usize) * 4]);
        }
        Rgba8::from_pixels(size, size, pixels)
    }

    pub fn decode_png(bytes: &[u8]) -> Result<Self, String> {
        let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
        // 调色板/灰度/低位深一律展开成 RGBA8（图标资源里确实有 Indexed PNG）。
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = decoder.read_info().map_err(|error| error.to_string())?;
        let mut buffer = vec![0; reader.output_buffer_size()];
        let info = reader
            .next_frame(&mut buffer)
            .map_err(|error| error.to_string())?;
        buffer.truncate(info.buffer_size());
        // 统一成 RGBA8：灰度/调色板/16 位都归一化。
        let rgba = match (info.color_type, info.bit_depth) {
            (png::ColorType::Rgba, png::BitDepth::Eight) => buffer,
            _ => normalize_to_rgba8(&buffer, info.color_type, info.bit_depth)?,
        };
        Ok(Rgba8::from_pixels(info.width, info.height, rgba))
    }

    pub fn encode_png(&self) -> Result<Vec<u8>, String> {
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, self.width, self.height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().map_err(|error| error.to_string())?;
            writer
                .write_image_data(&self.pixels)
                .map_err(|error| error.to_string())?;
        }
        Ok(out)
    }

    /// 直通 → 预乘（RGB × A）。游戏导出的图标就是预乘的，比对时要转换。
    pub fn premultiplied(&self) -> Rgba8 {
        let mut pixels = self.pixels.clone();
        for chunk in pixels.chunks_exact_mut(4) {
            let alpha = chunk[3] as u32;
            for channel in &mut chunk[..3] {
                *channel = ((*channel as u32 * alpha + 127) / 255) as u8;
            }
        }
        Rgba8::from_pixels(self.width, self.height, pixels)
    }

    /// 预乘 → 直通（RGB ÷ A）；全透明像素保持全 0（无从还原，也无意义）。
    pub fn unpremultiplied(&self) -> Rgba8 {
        let mut pixels = self.pixels.clone();
        for chunk in pixels.chunks_exact_mut(4) {
            let alpha = chunk[3] as u32;
            if alpha == 0 {
                continue;
            }
            for channel in &mut chunk[..3] {
                *channel = (((*channel as u32 * 255) + alpha / 2) / alpha).min(255) as u8;
            }
        }
        Rgba8::from_pixels(self.width, self.height, pixels)
    }

    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let index = ((y * self.width + x) * 4) as usize;
        [
            self.pixels[index],
            self.pixels[index + 1],
            self.pixels[index + 2],
            self.pixels[index + 3],
        ]
    }

    /// 缩放（面积平均：缩小时不会丢细节、放大时是平滑插值）。
    ///
    /// Factorio 自己的重采样细节（滤波核、mipmap 选择）无法完全复刻，这里给一个
    /// 确定性的面积平均；`scale` 显式给出的层很少，偏差由比对工具量化。
    pub fn scaled(&self, scale: f64) -> Rgba8 {
        let new_w = ((self.width as f64 * scale).round() as u32).max(1);
        let new_h = ((self.height as f64 * scale).round() as u32).max(1);
        let mut out = Rgba8::transparent(new_w, new_h);
        let sx = self.width as f64 / new_w as f64;
        let sy = self.height as f64 / new_h as f64;
        for y in 0..new_h {
            let y0 = (y as f64 * sy) as u32;
            let y1 = (((y + 1) as f64 * sy).ceil() as u32)
                .min(self.height)
                .max(y0 + 1);
            for x in 0..new_w {
                let x0 = (x as f64 * sx) as u32;
                let x1 = (((x + 1) as f64 * sx).ceil() as u32)
                    .min(self.width)
                    .max(x0 + 1);
                // 预乘后再平均，避免透明像素的 RGB 污染边缘。
                let mut sum = [0f64; 4];
                let mut count = 0f64;
                for sy_index in y0..y1 {
                    for sx_index in x0..x1 {
                        let [r, g, b, a] = self.pixel(sx_index, sy_index);
                        let af = a as f64 / 255.0;
                        sum[0] += r as f64 * af;
                        sum[1] += g as f64 * af;
                        sum[2] += b as f64 * af;
                        sum[3] += a as f64;
                        count += 1.0;
                    }
                }
                if count == 0.0 {
                    continue;
                }
                let alpha = sum[3] / count;
                let straight = |value: f64| -> u8 {
                    if alpha <= 0.0 {
                        0
                    } else {
                        ((value / count) / (alpha / 255.0)).clamp(0.0, 255.0) as u8
                    }
                };
                out.pixels[((y * new_w + x) * 4) as usize] = straight(sum[0]);
                out.pixels[((y * new_w + x) * 4 + 1) as usize] = straight(sum[1]);
                out.pixels[((y * new_w + x) * 4 + 2) as usize] = straight(sum[2]);
                out.pixels[((y * new_w + x) * 4 + 3) as usize] = alpha.round() as u8;
            }
        }
        out
    }

    /// 双线性缩放（目标尺寸直接给出）。预乘域里插值，避免透明像素污染颜色。
    pub fn scaled_bilinear(&self, new_w: u32, new_h: u32) -> Rgba8 {
        let new_w = new_w.max(1);
        let new_h = new_h.max(1);
        if new_w == self.width && new_h == self.height {
            return self.clone();
        }
        let mut out = Rgba8::transparent(new_w, new_h);
        // 像素中心对齐：源坐标 = (目标坐标 + 0.5) * 源尺寸 / 目标尺寸 - 0.5。
        let ratio_x = self.width as f64 / new_w as f64;
        let ratio_y = self.height as f64 / new_h as f64;
        let sample = |x: f64, y: f64| -> [f64; 4] {
            let x = x.clamp(0.0, (self.width - 1) as f64);
            let y = y.clamp(0.0, (self.height - 1) as f64);
            let x0 = x.floor() as u32;
            let y0 = y.floor() as u32;
            let x1 = (x0 + 1).min(self.width - 1);
            let y1 = (y0 + 1).min(self.height - 1);
            let fx = x - x0 as f64;
            let fy = y - y0 as f64;
            let mut sum = [0.0f64; 4];
            for (px, py, weight) in [
                (x0, y0, (1.0 - fx) * (1.0 - fy)),
                (x1, y0, fx * (1.0 - fy)),
                (x0, y1, (1.0 - fx) * fy),
                (x1, y1, fx * fy),
            ] {
                let [r, g, b, a] = self.pixel(px, py);
                let alpha = a as f64 / 255.0;
                sum[0] += r as f64 * alpha * weight;
                sum[1] += g as f64 * alpha * weight;
                sum[2] += b as f64 * alpha * weight;
                sum[3] += a as f64 * weight;
            }
            sum
        };
        for y in 0..new_h {
            for x in 0..new_w {
                let sum = sample(
                    (x as f64 + 0.5) * ratio_x - 0.5,
                    (y as f64 + 0.5) * ratio_y - 0.5,
                );
                let alpha = sum[3].clamp(0.0, 255.0);
                let straight = |value: f64| -> u8 {
                    if alpha <= 0.0 {
                        0
                    } else {
                        (value / (alpha / 255.0)).clamp(0.0, 255.0).round() as u8
                    }
                };
                let index = ((y * new_w + x) * 4) as usize;
                out.pixels[index] = straight(sum[0]);
                out.pixels[index + 1] = straight(sum[1]);
                out.pixels[index + 2] = straight(sum[2]);
                out.pixels[index + 3] = alpha.round() as u8;
            }
        }
        out
    }

    /// 「mipmap 式」缩放：先按 2×2 逐级降到还不小于目标的 mip 级别，再双线性插值到目标尺寸。
    ///
    /// 这是想贴近 Factorio 的做法（它给贴图生成 mipmap，缩小时选级别再线性过滤）：
    /// 面积平均在 2 倍整数缩放时和它一致（都是 2×2 平均），非整数倍时两者的核不同。
    pub fn scaled_mipmap(&self, scale: f64) -> Rgba8 {
        let new_w = ((self.width as f64 * scale).round() as u32).max(1);
        let new_h = ((self.height as f64 * scale).round() as u32).max(1);
        if new_w == self.width && new_h == self.height {
            return self.clone();
        }
        let mut level = 0u32;
        while scale < 1.0
            && (self.width >> (level + 1)) >= new_w
            && (self.height >> (level + 1)) >= new_h
        {
            level += 1;
        }
        let mut base = self.clone();
        for _ in 0..level {
            base = base.scaled(0.5);
        }
        if base.width == new_w && base.height == new_h {
            base
        } else {
            base.scaled_bilinear(new_w, new_h)
        }
    }

    /// 把 `layer` 的左上角放到画布坐标 `(x, y)`（source-over）。坐标可以是负的（超出裁剪）。
    pub fn composite_at(&mut self, layer: &Rgba8, x: i32, y: i32) {
        for ly in 0..layer.height as i32 {
            let cy = y + ly;
            if cy < 0 || cy >= self.height as i32 {
                continue;
            }
            for lx in 0..layer.width as i32 {
                let cx = x + lx;
                if cx < 0 || cx >= self.width as i32 {
                    continue;
                }
                let [sr, sg, sb, sa] = layer.pixel(lx as u32, ly as u32);
                if sa == 0 {
                    continue;
                }
                let index = ((cy as u32 * self.width + cx as u32) * 4) as usize;
                let [dr, dg, db, da] = [
                    self.pixels[index],
                    self.pixels[index + 1],
                    self.pixels[index + 2],
                    self.pixels[index + 3],
                ];
                if da == 255 && sa == 255 {
                    self.pixels[index] = sr;
                    self.pixels[index + 1] = sg;
                    self.pixels[index + 2] = sb;
                    self.pixels[index + 3] = 255;
                    continue;
                }
                // 预乘域里做 source-over，再转回直通。
                let sa_f = sa as f64 / 255.0;
                let da_f = da as f64 / 255.0;
                let out_a = sa_f + da_f * (1.0 - sa_f);
                let blend = |src: u8, dst: u8| -> u8 {
                    let src_pre = src as f64 * sa_f;
                    let dst_pre = dst as f64 * da_f;
                    let out_pre = src_pre + dst_pre * (1.0 - sa_f);
                    if out_a <= 0.0 {
                        0
                    } else {
                        (out_pre / out_a).clamp(0.0, 255.0).round() as u8
                    }
                };
                self.pixels[index] = blend(sr, dr);
                self.pixels[index + 1] = blend(sg, dg);
                self.pixels[index + 2] = blend(sb, db);
                self.pixels[index + 3] = (out_a * 255.0).round() as u8;
            }
        }
    }

    /// 把 `layer` **居中**叠到本画布上，再整体偏移 `offset_x/offset_y` 像素（source-over）。
    pub fn composite_over_centered(&mut self, layer: &Rgba8, offset_x: i32, offset_y: i32) {
        let base_x = (self.width as i32 - layer.width as i32) / 2 + offset_x;
        let base_y = (self.height as i32 - layer.height as i32) / 2 + offset_y;
        self.composite_at(layer, base_x, base_y);
    }
}

fn normalize_to_rgba8(
    buffer: &[u8],
    color_type: png::ColorType,
    bit_depth: png::BitDepth,
) -> Result<Vec<u8>, String> {
    // 只支持 8 位输入；其它位深先报错（图标资源实际都是 8 位 RGBA）。
    if bit_depth != png::BitDepth::Eight {
        return Err(format!("暂不支持的 PNG 位深: {bit_depth:?}"));
    }
    let mut out = Vec::new();
    let channels = match color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        other => return Err(format!("暂不支持的 PNG 颜色类型: {other:?}")),
    };
    for chunk in buffer.chunks_exact(channels) {
        match color_type {
            png::ColorType::Grayscale => {
                out.extend_from_slice(&[chunk[0], chunk[0], chunk[0], 255])
            }
            png::ColorType::GrayscaleAlpha => {
                out.extend_from_slice(&[chunk[0], chunk[0], chunk[0], chunk[1]])
            }
            png::ColorType::Rgb => out.extend_from_slice(&[chunk[0], chunk[1], chunk[2], 255]),
            png::ColorType::Rgba => out.extend_from_slice(chunk),
            _ => unreachable!(),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn premultiply_round_trip_is_close() {
        let image = Rgba8::from_pixels(
            2,
            1,
            vec![
                200, 100, 50, 128, //
                255, 255, 255, 255,
            ],
        );
        let pre = image.premultiplied();
        assert_eq!(pre.pixel(0, 0), [100, 50, 25, 128]);
        assert_eq!(pre.pixel(1, 0), [255, 255, 255, 255]);
        let back = pre.unpremultiplied();
        // 预乘会丢精度，允许 ±2。
        let [r, g, b, _] = back.pixel(0, 0);
        assert!(
            (r as i32 - 200).abs() <= 2
                && (g as i32 - 100).abs() <= 2
                && (b as i32 - 50).abs() <= 2
        );
    }

    #[test]
    fn top_left_tile_crops_mipmap_strip() {
        // 4×2 的图，取左上 2×2 应该拿到前两列。
        let image = Rgba8::from_pixels(
            4,
            2,
            vec![
                1, 1, 1, 255, 2, 2, 2, 255, 9, 9, 9, 255, 9, 9, 9, 255, //
                3, 3, 3, 255, 4, 4, 4, 255, 9, 9, 9, 255, 9, 9, 9, 255,
            ],
        );
        let tile = image.top_left_tile(2);
        assert_eq!(tile.width, 2);
        assert_eq!(tile.height, 2);
        assert_eq!(tile.pixel(0, 0), [1, 1, 1, 255]);
        assert_eq!(tile.pixel(1, 1), [4, 4, 4, 255]);
    }
}
