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
//!
//! **没有图标定义时的推导**：官方只对两种原型写明了推导规则（官方 schema 全文只有这两处，
//! 见 `crates/metatorio-data/schema/prototype-api.json`）：
//! - 配方：用 `main_product` 或**唯一产物**的图标（[`derived_product`]）；
//! - 地块：用 `variants.material_background`（**还没实现**）。
//!
//! 其余类型官方写的是「图标必填」或压根没有图标字段，没有推导可做；item → `place_result`
//! 这条**官方文档里没有**（`ItemPrototype::icon` 是 "Only loaded, and mandatory if `icons`
//! is not defined"），实测两个上下文里也没有任何一个物品缺图标定义，所以不实现。

use metatorio_data::store::{PrototypeGroup, PrototypeRecord, PrototypeStore};
use metatorio_data::types::Product;
use metatorio_data::{IconComponent, IconData, RecipeComponent};

use std::path::Path;

use crate::image::Rgba8;
use crate::sources::IconSources;

/// 渲染失败的原因（都能定位到具体原型/层，便于统计「缺文件」有多少）。
#[derive(Debug, Clone)]
pub enum IconRenderError {
    /// 既没有 `icon` 也没有 `icons`，也**没有可用的推导**（`detail` 说明是哪种情形）。
    NoIcon { detail: String },
    /// 推导出来的产物原型本身拿不到图标（找不到，或它自己也没有图标定义）。
    DerivedMissing { product: String, detail: String },
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
            Self::NoIcon { detail } => write!(f, "该原型没有图标定义，也没有可用推导: {detail}"),
            Self::DerivedMissing { product, detail } => {
                write!(f, "推导产物 {product} 失败: {detail}")
            }
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
///
/// **不带推导**：只按原型自己的 `icon`/`icons` 画。要按官方规则推导（配方 → 主产物）
/// 用 [`render_record_icon`]。
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
        .ok_or_else(|| IconRenderError::NoIcon {
            detail: "没有 IconComponent".to_string(),
        })?;
    render_icon_with(
        component,
        expected_icon_size(&record.type_),
        sources,
        options,
    )
}

/// 原型是否自带图标定义（`icons` 或 `icon`）。
pub fn has_icon_definition(record: &PrototypeRecord) -> bool {
    record
        .component::<IconComponent>()
        .is_some_and(|component| !(component.icons.is_empty() && component.icon.is_none()))
}

/// 官方文档写明的图标推导：**配方**没有 `icons`/`icon` 时，用 `main_product` 或**唯一产物**
/// 的图标（item 优先，其次 fluid）。返回被借用的产物原型。
///
/// 出处：`RecipePrototype::icon` —— "If given, this determines the recipe's icon. Otherwise,
/// the icon of `main_product` or the singular product is used. … Mandatory if `icons` is not
/// defined for a recipe with more than one product and no `main_product`, or no product."
///
/// 其他类型官方没有推导规则，一律返回 `None`。
pub fn derived_product<'a>(
    store: &'a PrototypeStore,
    record: &PrototypeRecord,
) -> Option<&'a PrototypeRecord> {
    if record.type_ != "recipe" {
        return None;
    }
    let recipe = record.component::<RecipeComponent>()?;
    let product = match recipe
        .main_product
        .as_deref()
        .filter(|name| !name.is_empty())
    {
        Some(main) => main,
        // 没有 main_product 时只有「唯一产物」能推导；多产物必须显式给 icon。
        None if recipe.results.len() == 1 => product_name(recipe.results.first()?),
        None => return None,
    };
    store
        .get(PrototypeGroup::Item, product)
        .or_else(|| store.get(PrototypeGroup::Fluid, product))
}

/// 渲染一个原型的图标，**含官方写明的推导**（配方 → 主产物/唯一产物）。
pub fn render_record_icon(
    store: &PrototypeStore,
    record: &PrototypeRecord,
    sources: &IconSources,
) -> Result<Rgba8, IconRenderError> {
    render_record_icon_with(store, record, sources, RenderOptions::default())
}

/// 带参数的版本（定标/试验用）。
pub fn render_record_icon_with(
    store: &PrototypeStore,
    record: &PrototypeRecord,
    sources: &IconSources,
    options: RenderOptions,
) -> Result<Rgba8, IconRenderError> {
    let expected = expected_icon_size(&record.type_);
    if has_icon_definition(record) {
        let component = record
            .component::<IconComponent>()
            .expect("has_icon_definition 已确认组件存在");
        return render_icon_with(component, expected, sources, options);
    }
    let product = derived_product(store, record).ok_or_else(|| IconRenderError::NoIcon {
        detail: no_icon_detail(record),
    })?;
    let component = product
        .component::<IconComponent>()
        .filter(|component| !(component.icons.is_empty() && component.icon.is_none()))
        .ok_or_else(|| IconRenderError::DerivedMissing {
            product: product.name.clone(),
            detail: format!("{}/{} 自己也没有图标定义", product.type_, product.name),
        })?;
    // 画布尺寸按**配方**的规格（产物是 item/fluid，默认同为 64）。
    render_icon_with(component, expected, sources, options)
}

fn product_name(product: &Product) -> &str {
    match product {
        Product::Item(item) => &item.name,
        Product::Fluid(fluid) => &fluid.name,
    }
}

/// 说明「为什么推不出来」，让失败信息可定位。
fn no_icon_detail(record: &PrototypeRecord) -> String {
    if record.type_ != "recipe" {
        return format!("{} 类型官方没有推导规则", record.type_);
    }
    let Some(recipe) = record.component::<RecipeComponent>() else {
        return "没有 RecipeComponent".to_string();
    };
    let main = recipe
        .main_product
        .as_deref()
        .filter(|name| !name.is_empty());
    match (main, recipe.results.len()) {
        (None, 0) => "配方没有产物".to_string(),
        (None, count) if count > 1 => format!("配方有 {count} 个产物且没有 main_product"),
        (Some(name), _) => format!("main_product {name} 在仓库里找不到"),
        (None, _) => "唯一产物在仓库里找不到".to_string(),
    }
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
        return Err(IconRenderError::NoIcon {
            detail: "IconComponent 里既没有 icons 也没有 icon".to_string(),
        });
    }
    let expected = canvas_size(component, type_default_size);
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
    /// 其中靠**推导**写出的张数（配方 → 主产物/唯一产物）。
    pub written_derived: usize,
    /// 没有图标定义、也没有可用推导（例如多产物且没有 `main_product` 的配方）。
    pub no_icon: usize,
    /// 推导指向的产物原型自己拿不到图标。
    pub derived_missing: usize,
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
        self.no_icon + self.derived_missing + self.missing_source + self.decode_failed
    }

    fn record_failure(&mut self, record: &PrototypeRecord, error: &IconRenderError) {
        match error {
            IconRenderError::NoIcon { .. } => self.no_icon += 1,
            IconRenderError::DerivedMissing { .. } => self.derived_missing += 1,
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

/// 把一个原型仓库里**所有能画出图标的原型**渲染到 `out_dir/<type>/<name>.png`。
///
/// 画不出来的分三类如实计数：没有图标定义且没有推导（`no_icon`）、推导出的产物拿不到图标
/// （`derived_missing`）、缺文件/解码失败。**不允许静默缺图**。
///
/// 输出布局与游戏 `--dump-icon-sprites` 一致，因此前端 `icon` 命令与缓存目录约定都不用改。
pub fn render_all_icons(
    store: &metatorio_data::store::PrototypeStore,
    sources: &IconSources,
    out_dir: &Path,
    options: RenderOptions,
) -> Result<RenderReport, String> {
    let mut report = RenderReport::default();
    for records in store.groups.values() {
        for record in records.values() {
            let derived = !has_icon_definition(record);
            match render_record_icon_with(store, record, sources, options) {
                Ok(image) => {
                    let dir = out_dir.join(&record.type_);
                    std::fs::create_dir_all(&dir)
                        .map_err(|error| format!("创建 {} 失败: {error}", dir.display()))?;
                    let path = dir.join(format!("{}.png", record.name));
                    let bytes = image.encode_png()?;
                    std::fs::write(&path, bytes)
                        .map_err(|error| format!("写 {} 失败: {error}", path.display()))?;
                    report.written += 1;
                    if derived {
                        report.written_derived += 1;
                    }
                    *report.by_type.entry(record.type_.clone()).or_insert(0) += 1;
                }
                Err(error) => report.record_failure(record, &error),
            }
        }
    }
    Ok(report)
}

/// 画布边长。
///
/// 官方 `icon_size` 的说明是 "Only loaded if `icons` is not defined"——**给了 `icons` 时，
/// 原型上的 `icon_size` 根本不参与**，画布取**第 0 层**的层边长。实测三例（py）：
/// `icon_size = 32` + 层 `[256, 32]` → 官方 256×256；层 `[64, 32]` → 64×64；
/// 层 `[1, 32]`（pyvoid 的占位底层）→ 官方就是 **1×1**。所以既不是原型的 `icon_size`、
/// 也不是「最大的层」（按原型的 32 去比，整幅图只剩 1% 对得上）。
/// 没有 `icons`、只有 `icon` 时，才用原型的 `icon_size`。
fn canvas_size(component: &IconComponent, type_default_size: u32) -> u32 {
    if let Some(first) = component.icons.first() {
        return first
            .icon_size
            .map(|size| size.max(1) as u32)
            .unwrap_or(type_default_size)
            .max(1);
    }
    component
        .icon_size
        .map(|size| size.max(1) as u32)
        .unwrap_or(type_default_size)
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

    /// 配方图标推导：`main_product` 优先；没有它时只有**唯一产物**能推导；多产物必须显式给。
    #[test]
    fn recipe_derivation_follows_the_documented_rule() {
        let store = crate::test_support::store_from_json(serde_json::json!({
            "item": {
                "iron-plate": {
                    "type": "item",
                    "name": "iron-plate",
                    "icon": "__base__/graphics/icons/iron-plate.png"
                },
                "copper-plate": {
                    "type": "item",
                    "name": "copper-plate",
                    "icon": "__base__/graphics/icons/copper-plate.png"
                }
            },
            "fluid": {
                "water": {
                    "type": "fluid",
                    "name": "water",
                    "icon": "__base__/graphics/icons/fluid/water.png"
                }
            },
            "recipe": {
                "single": {
                    "type": "recipe",
                    "name": "single",
                    "results": [{ "type": "item", "name": "iron-plate", "amount": 1 }]
                },
                "single-fluid": {
                    "type": "recipe",
                    "name": "single-fluid",
                    "results": [{ "type": "fluid", "name": "water", "amount": 10 }]
                },
                "main-product": {
                    "type": "recipe",
                    "name": "main-product",
                    "main_product": "copper-plate",
                    "results": [
                        { "type": "item", "name": "iron-plate", "amount": 1 },
                        { "type": "item", "name": "copper-plate", "amount": 1 }
                    ]
                },
                "multi-no-main": {
                    "type": "recipe",
                    "name": "multi-no-main",
                    "results": [
                        { "type": "item", "name": "iron-plate", "amount": 1 },
                        { "type": "item", "name": "copper-plate", "amount": 1 }
                    ]
                },
                "explicit": {
                    "type": "recipe",
                    "name": "explicit",
                    "icon": "__base__/graphics/icons/iron-gear-wheel.png",
                    "results": [{ "type": "item", "name": "iron-plate", "amount": 1 }]
                }
            }
        }));
        let recipe = |name: &str| {
            store
                .groups
                .values()
                .flat_map(|records| records.values())
                .find(|record| record.type_ == "recipe" && record.name == name)
                .expect("配方存在")
        };
        let derived = |name: &str| derived_product(&store, recipe(name)).map(|p| p.name.clone());

        assert_eq!(derived("single").as_deref(), Some("iron-plate"));
        assert_eq!(derived("single-fluid").as_deref(), Some("water"));
        // main_product 优先于「唯一产物」之外的多产物
        assert_eq!(derived("main-product").as_deref(), Some("copper-plate"));
        // 多产物且没有 main_product → 官方要求显式给 icon，不推导
        assert_eq!(derived("multi-no-main"), None);
        // 自己有图标定义时不会走推导（是否走推导由 render_record_icon_with 判断），
        // 但 derived_product 本身是纯查询：只看类型与产物。
        assert!(has_icon_definition(recipe("explicit")));
        assert_eq!(derived("explicit").as_deref(), Some("iron-plate"));
        // 其他类型没有推导规则
        let item = store
            .get(metatorio_data::store::PrototypeGroup::Item, "iron-plate")
            .expect("物品存在");
        assert!(derived_product(&store, item).is_none());
    }

    /// 画布口径：给了 `icons` 就**不看**原型的 `icon_size`，取**第 0 层**的层边长
    /// （py 的配方是 `icon_size = 32` + 层 256/32 → 官方 256×256；pyvoid 是层 1/32 → 官方 1×1）。
    #[test]
    fn canvas_size_ignores_prototype_icon_size_when_icons_exist() {
        let component = IconComponent {
            icon: None,
            icon_size: Some(32),
            icons: vec![
                IconData {
                    icon: "a.png".to_string(),
                    icon_size: Some(256),
                    ..IconData::default()
                },
                IconData {
                    icon: "b.png".to_string(),
                    icon_size: Some(32),
                    ..IconData::default()
                },
            ],
        };
        assert_eq!(canvas_size(&component, 64), 256);

        // 第 0 层只有 1 像素、后面跟着 32 像素的叠加层：官方导出就是 1×1
        let component = IconComponent {
            icon: None,
            icon_size: Some(32),
            icons: vec![
                IconData {
                    icon: "a.png".to_string(),
                    icon_size: Some(1),
                    ..IconData::default()
                },
                IconData {
                    icon: "b.png".to_string(),
                    icon_size: Some(32),
                    ..IconData::default()
                },
            ],
        };
        assert_eq!(canvas_size(&component, 64), 1);

        // 层没写 icon_size → 用类型默认
        let component = IconComponent {
            icon: None,
            icon_size: Some(32),
            icons: vec![IconData {
                icon: "a.png".to_string(),
                ..IconData::default()
            }],
        };
        assert_eq!(canvas_size(&component, 64), 64);

        // 只有 legacy `icon` → 用原型的 icon_size
        let component = IconComponent {
            icon: Some("a.png".to_string()),
            icon_size: Some(128),
            icons: Vec::new(),
        };
        assert_eq!(canvas_size(&component, 64), 128);
    }
}
