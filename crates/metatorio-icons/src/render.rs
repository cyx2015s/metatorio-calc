//! 按官方 `IconData` 规则渲染图标。
//!
//! 规则（来自官方文档 <https://lua-api.factorio.com/2.1.19/types/IconData.html>）：
//! - **层序**：数组顺序即绘制顺序，后面的层盖在前面的层上；
//! - **`icon_size`**：该层自己的方形边长（默认 64）；图标文件是「mipmap 横排」，
//!   level 0 就是左上角的 `icon_size × icon_size`；
//! - **`scale`**：显式缩放；没有显式给定时官方默认 `(expected_icon_size / 2) / icon_size`
//!   （见 [`layer_scale`]，实测口径以官方导出为准）；
//! - **`shift`**：以「整体图标被假定为 `expected_icon_size / 2` 像素宽高」为单位，
//!   从中心偏移（`{0, expected/2}` = 整整往下挪一个图标高度 ⇒ 1 单位 = 2 像素）；
//! - **`tint`**：整层乘色（含 alpha）；
//! - **`draw_background`**：默认为**首层**画描边/阴影——但实测 `--dump-icon-sprites`
//!   的输出里**没有**描边（导出 ≈ 源图预乘 alpha），所以本实现不画；
//! - **`floating`**：不参与「整组图标取包围盒」，画布固定时无需处理。
//!
//! 画布大小取该原型的 `icon_size`（没给则按类型默认，见 [`expected_icon_size`]）。

use metatorio_data::store::PrototypeRecord;
use metatorio_data::{IconComponent, IconData};

use std::path::Path;

use crate::image::Rgba8;
use crate::sources::IconSources;

/// 渲染失败的原因（都能定位到具体原型/层，便于统计「缺文件」有多少）。
#[derive(Debug, Clone)]
pub enum IconRenderError {
    /// 既没有 `icon` 也没有 `icons`。
    NoIcon,
    /// 某个层的文件读不到。
    MissingSource {
        layer: usize,
        spec: String,
        error: String,
    },
    /// 某个层的 PNG 解不开。
    Decode {
        layer: usize,
        spec: String,
        error: String,
    },
}

impl std::fmt::Display for IconRenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoIcon => write!(f, "该原型没有图标定义"),
            Self::MissingSource { layer, spec, error } => {
                write!(f, "第 {layer} 层读不到 {spec}: {error}")
            }
            Self::Decode { layer, spec, error } => {
                write!(f, "第 {layer} 层解码 {spec} 失败: {error}")
            }
        }
    }
}

impl std::error::Error for IconRenderError {}

/// 类型的默认「期望图标边长」（官方文档的 Expected icon sizes）。
///
/// 官方文档的 Expected icon sizes 里，512 只针对 `SpaceLocationPrototype::starmap_icon`、
/// 32 只针对 `ShortcutPrototype::small_icons`——普通 `icons` 一律按 64（科技 256、
/// 成就/物品组 128 是它们**本身**的规格）。原型自己给了 `icon_size` 时以原型为准。
pub fn expected_icon_size(type_: &str) -> u32 {
    match type_ {
        "technology" => 256,
        "achievement" | "item-group" => 128,
        _ => 64,
    }
}

/// 层的尺寸口径（拿官方导出比对来定标：`examples/compare.rs --sweep`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScaleLaw {
    /// `icon_size × scale.unwrap_or(1)`（默认）
    Natural,
    /// `icon_size × scale.unwrap_or(1) × 2`
    NaturalDouble,
    /// 忽略 `scale`，永远画 `icon_size`
    IgnoreScale,
    /// `expected × scale.unwrap_or(1)`
    ExpectedTimesScale,
    /// 官方文档写的默认：`scale.unwrap_or((expected/2)/icon_size)`
    DocDefault,
}

impl ScaleLaw {
    pub fn all() -> [ScaleLaw; 5] {
        [
            ScaleLaw::Natural,
            ScaleLaw::NaturalDouble,
            ScaleLaw::IgnoreScale,
            ScaleLaw::ExpectedTimesScale,
            ScaleLaw::DocDefault,
        ]
    }

    pub fn name(self) -> &'static str {
        match self {
            ScaleLaw::Natural => "icon_size × scale",
            ScaleLaw::NaturalDouble => "icon_size × scale × 2",
            ScaleLaw::IgnoreScale => "icon_size（忽略 scale）",
            ScaleLaw::ExpectedTimesScale => "expected × scale",
            ScaleLaw::DocDefault => "scale.unwrap_or((expected/2)/icon_size)",
        }
    }
}

/// 渲染参数（默认 = 当前模型；由 sweep 定标后写回默认值）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderOptions {
    pub scale_law: ScaleLaw,
    /// `shift` 的像素换算：1 单位 = 这么多像素（官方文档说 1 单位 = 2 像素）。
    pub shift_pixels_per_unit: f64,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            scale_law: ScaleLaw::Natural,
            shift_pixels_per_unit: 2.0,
        }
    }
}

/// 渲染一个原型的图标（画布边长 = 原型 `icon_size`，否则按类型默认）。
pub fn render_prototype_icon(
    record: &PrototypeRecord,
    sources: &IconSources,
) -> Result<Rgba8, IconRenderError> {
    render_prototype_icon_with(record, sources, RenderOptions::default())
}

/// 带参数的版本（定标/试验用）。
pub fn render_prototype_icon_with(
    record: &PrototypeRecord,
    sources: &IconSources,
    options: RenderOptions,
) -> Result<Rgba8, IconRenderError> {
    let component = record
        .component::<IconComponent>()
        .ok_or(IconRenderError::NoIcon)?;
    render_icon_with(
        component,
        expected_icon_size(&record.type_),
        sources,
        options,
    )
}

/// 渲染一个 `IconComponent`（默认参数）。
pub fn render_icon(
    component: &IconComponent,
    type_default_size: u32,
    sources: &IconSources,
) -> Result<Rgba8, IconRenderError> {
    render_icon_with(
        component,
        type_default_size,
        sources,
        RenderOptions::default(),
    )
}

/// 渲染一个 `IconComponent`（带参数）。
pub fn render_icon_with(
    component: &IconComponent,
    type_default_size: u32,
    sources: &IconSources,
    options: RenderOptions,
) -> Result<Rgba8, IconRenderError> {
    let layers = layers_of(component, type_default_size);
    if layers.is_empty() {
        return Err(IconRenderError::NoIcon);
    }
    let expected = component
        .icon_size
        .map(|size| size.max(1) as u32)
        .unwrap_or(type_default_size);
    let mut canvas = Rgba8::transparent(expected, expected);
    for (index, layer) in layers.iter().enumerate() {
        let layer_size = layer
            .icon_size
            .map(|size| size.max(1) as u32)
            .unwrap_or(expected);
        let bytes = sources
            .read(&layer.icon)
            .map_err(|error| IconRenderError::MissingSource {
                layer: index,
                spec: layer.icon.clone(),
                error,
            })?;
        let source = Rgba8::decode_png(&bytes).map_err(|error| IconRenderError::Decode {
            layer: index,
            spec: layer.icon.clone(),
            error,
        })?;
        let mut tile = source.top_left_tile(layer_size);
        apply_tint(&mut tile, layer.tint);
        let drawn = layer_drawn_size(layer, layer_size, expected, options.scale_law);
        let scale = drawn as f64 / tile.width as f64;
        if (scale - 1.0).abs() > f64::EPSILON {
            tile = tile.scaled(scale);
        }
        let shift = layer.shift.unwrap_or_default();
        let dx = shift.0 * options.shift_pixels_per_unit;
        let dy = shift.1 * options.shift_pixels_per_unit;
        canvas.composite_over_centered(&tile, dx.round() as i32, dy.round() as i32);
    }
    Ok(canvas)
}

/// 批量渲染的结果（必须如实上报：缺文件/解码失败不能被当成「就这么多图标」）。
#[derive(Debug, Default, Clone)]
pub struct RenderReport {
    /// 成功写出的图标数。
    pub written: usize,
    /// 原型没有图标定义（游戏会按产物自动生成配方图标，这类暂时跳过）。
    pub no_icon: usize,
    /// 层引用的文件读不到。
    pub missing_source: usize,
    /// 层文件解码失败。
    pub decode_failed: usize,
    /// 前若干条失败样本（`type/name: 原因`），便于排查。
    pub samples: Vec<String>,
    /// 按类型统计写出数（`item` → 1234）。
    pub by_type: std::collections::BTreeMap<String, usize>,
}

impl RenderReport {
    pub fn failed(&self) -> usize {
        self.no_icon + self.missing_source + self.decode_failed
    }

    fn record_failure(&mut self, record: &PrototypeRecord, error: &IconRenderError) {
        match error {
            IconRenderError::NoIcon => self.no_icon += 1,
            IconRenderError::MissingSource { .. } => self.missing_source += 1,
            IconRenderError::Decode { .. } => self.decode_failed += 1,
        }
        const MAX_SAMPLES: usize = 10;
        if self.samples.len() < MAX_SAMPLES {
            self.samples
                .push(format!("{}/{}: {error}", record.type_, record.name));
        }
    }
}

/// 把一个原型仓库里**所有带图标定义的原型**渲染到 `out_dir/<type>/<name>.png`。
///
/// 输出布局与游戏 `--dump-icon-sprites` 一致，因此前端 `icon` 命令与缓存目录约定都不用改。
/// 返回的成功/失败计数由调用方如实上报（**不允许静默缺图**）。
pub fn render_all_icons(
    store: &metatorio_data::store::PrototypeStore,
    sources: &IconSources,
    out_dir: &Path,
    options: RenderOptions,
) -> Result<RenderReport, String> {
    let mut report = RenderReport::default();
    for records in store.groups.values() {
        for record in records.values() {
            let Some(component) = record.component::<IconComponent>() else {
                continue;
            };
            if component.icons.is_empty() && component.icon.is_none() {
                continue;
            }
            match render_prototype_icon_with(record, sources, options) {
                Ok(image) => {
                    let dir = out_dir.join(&record.type_);
                    std::fs::create_dir_all(&dir)
                        .map_err(|error| format!("创建 {} 失败: {error}", dir.display()))?;
                    let path = dir.join(format!("{}.png", record.name));
                    let bytes = image.encode_png()?;
                    std::fs::write(&path, bytes)
                        .map_err(|error| format!("写 {} 失败: {error}", path.display()))?;
                    report.written += 1;
                    *report.by_type.entry(record.type_.clone()).or_insert(0) += 1;
                }
                Err(error) => report.record_failure(record, &error),
            }
        }
    }
    Ok(report)
}

/// 层清单：`icons` 优先；只有 `icon` 时视为单层（`icon_size` 取原型的）。
fn layers_of(component: &IconComponent, type_default_size: u32) -> Vec<IconData> {
    if !component.icons.is_empty() {
        return component.icons.clone();
    }
    match &component.icon {
        Some(icon) => vec![IconData {
            icon: icon.clone(),
            icon_size: component.icon_size,
            ..IconData::default()
        }],
        None => {
            let _ = type_default_size;
            Vec::new()
        }
    }
}

/// 层在画布上的绘制边长（像素）。
fn layer_drawn_size(layer: &IconData, layer_size: u32, expected: u32, law: ScaleLaw) -> u32 {
    let natural = layer_size as f64;
    let size = match law {
        ScaleLaw::Natural => natural * layer.scale.unwrap_or(1.0),
        ScaleLaw::NaturalDouble => natural * layer.scale.unwrap_or(1.0) * 2.0,
        ScaleLaw::IgnoreScale => natural,
        ScaleLaw::ExpectedTimesScale => expected as f64 * layer.scale.unwrap_or(1.0),
        ScaleLaw::DocDefault => {
            let default = (expected as f64 / 2.0) / natural.max(1.0);
            natural * layer.scale.unwrap_or(default)
        }
    };
    size.round().max(1.0) as u32
}

/// 乘色：RGB 按 tint 的比例、alpha 乘 tint.a。
fn apply_tint(image: &mut Rgba8, tint: Option<metatorio_data::Color>) {
    let Some(tint) = tint else { return };
    let metatorio_data::Color(r, g, b, a) = tint;
    if (r, g, b, a) == (255, 255, 255, 255) {
        return;
    }
    for chunk in image.pixels.chunks_exact_mut(4) {
        chunk[0] = mul_div_255(chunk[0], r);
        chunk[1] = mul_div_255(chunk[1], g);
        chunk[2] = mul_div_255(chunk[2], b);
        chunk[3] = mul_div_255(chunk[3], a);
    }
}

fn mul_div_255(value: u8, factor: u8) -> u8 {
    ((value as u32 * factor as u32 + 127) / 255) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use metatorio_data::Color;

    #[test]
    fn expected_sizes_follow_the_official_table() {
        // 512 只属于 `starmap_icon`、32 只属于 `small_icons`：普通 icons 一律 64。
        assert_eq!(expected_icon_size("technology"), 256);
        assert_eq!(expected_icon_size("achievement"), 128);
        assert_eq!(expected_icon_size("item-group"), 128);
        assert_eq!(expected_icon_size("space-location"), 64);
        assert_eq!(expected_icon_size("shortcut"), 64);
        assert_eq!(expected_icon_size("space-connection"), 64);
        assert_eq!(expected_icon_size("item"), 64);
        assert_eq!(expected_icon_size("recipe"), 64);
    }

    #[test]
    fn tint_multiplies_rgb_and_alpha() {
        let mut image = Rgba8::from_pixels(1, 1, vec![200, 100, 50, 200]);
        apply_tint(&mut image, Some(Color(0, 128, 255, 128)));
        // 200*0=0，100*128/255≈50，50*255/255=50，200*128/255≈100
        assert_eq!(image.pixel(0, 0), [0, 50, 50, 100]);
    }
}
