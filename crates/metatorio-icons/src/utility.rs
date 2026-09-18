//! 非原型图标：`utility-sprites` 里那些「不属于任何原型」的 GUI 素材。
//!
//! 组装机的选燃料按钮、插件槽 / 弹药槽 / 装甲槽 / 机器人槽的空槽背景、`fuel_icon`、
//! `ammo_icon` 这些都在 `utility-sprites` 里（**单个**原型实例，577 个具名字段），
//! 不在普通原型体系里，所以 `metatorio-data` 的关注类型里没有它。
//!
//! 这里直接从 dump 的 `utility-sprites` 节点读：字段值就是一个 Sprite
//! （`filename` + `size`，或 `width`/`height`，可选 `x`/`y`/`scale`/`tint`），
//! 形状实测分布：`Sprite(width/height)` 424、`Sprite(size)` 147、带 `layers` 4、其它 2。
//! 支持的形态占 99%，剩下的**如实计数**（不猜、不静默丢）。
//!
//! 输出到 `<图标目录>/utility/<字段名>.png`，与原型图标同一套读取路径
//! （应用里的 `icon` 命令就是按 `<ty>/<name>.png` 取），前端用 `type: "utility"` 请求。
//!
//! **没有官方参考图可比对**：游戏的 `--dump-icon-sprites` 只导出原型图标，不含
//! `utility-sprites`。所以这类图标的验证方式是「按定义裁切」＋人眼看（见 `docs/icon-rendering.md`）。

use std::path::Path;

use metatorio_data::Color;
use serde_json::Value;

use crate::sources::{IconSources, SourceError};

/// 一个可渲染的 GUI 素材。
#[derive(Debug, Clone, PartialEq)]
pub struct UtilitySprite {
    /// `utility-sprites` 里的字段名（例如 `empty_module_slot`）。
    pub name: String,
    /// 资源路径（`__core__/graphics/icons/mip/empty-module-slot.png`）。
    pub filename: String,
    /// 从素材里裁切的位置与大小（像素）。
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    /// 素材自带的缩放（例如 `0.5`）。
    pub scale: Option<f64>,
    /// 素材自带的染色。
    pub tint: Option<Color>,
}

/// 解析结果：能画的 + 跳过的（带原因，便于如实上报）。
#[derive(Debug, Default, Clone)]
pub struct UtilityParse {
    pub sprites: Vec<UtilitySprite>,
    /// 跳过原因（`字段名: 原因`）。
    pub skipped: Vec<String>,
}

/// 从 dump 里读 `utility-sprites`。没有这个节点（老 dump）时返回空，不报错。
pub fn parse_utility_sprites(dump: &Value) -> UtilityParse {
    let mut out = UtilityParse::default();
    let Some(section) = dump.get("utility-sprites").and_then(Value::as_object) else {
        return out;
    };
    // dump 的实际形状是 **`utility-sprites` → 原型（名字是 `default`）→ 577 个具名素材字段**，
    // 所以这里遍历「原型集合里的每个原型对象」，把对象里带 `filename` 的字段当素材读。
    for prototype in section.values() {
        let Some(fields) = prototype.as_object() else {
            continue;
        };
        for (name, value) in fields {
            // 原型自身的基础字段不是素材。
            if name == "type" || name == "name" {
                continue;
            }
            match sprite_of(value) {
                Some((filename, x, y, width, height, scale, tint)) => {
                    out.sprites.push(UtilitySprite {
                        name: name.clone(),
                        filename,
                        x,
                        y,
                        width,
                        height,
                        scale,
                        tint,
                    })
                }
                None => out
                    .skipped
                    .push(format!("{name}: 不是可渲染的 Sprite 形态")),
            }
        }
    }
    out
}

type SpriteFields = (String, u32, u32, u32, u32, Option<f64>, Option<Color>);

fn sprite_of(value: &Value) -> Option<SpriteFields> {
    // SpriteVariations（数组）：取第一个——图标用途下差异只是随机变体。
    let value = match value {
        Value::Array(items) => items.first()?,
        other => other,
    };
    let object = value.as_object()?;
    let filename = object.get("filename")?.as_str()?.to_string();
    let number = |key: &str| -> Option<f64> {
        match object.get(key)? {
            Value::Number(number) => number.as_f64(),
            Value::String(text) => text.parse::<f64>().ok(),
            _ => None,
        }
    };
    // 尺寸两种写法：`size: 64`（正方）或 `size: [w, h]`；也接受 `width`/`height`。
    let size_pair = |key: &str| -> Option<(f64, f64)> {
        match object.get(key)? {
            Value::Array(items) => {
                let first = items.first()?.as_f64()?;
                let second = items.get(1).and_then(Value::as_f64).unwrap_or(first);
                Some((first, second))
            }
            other => {
                let value = other.as_f64()?;
                Some((value, value))
            }
        }
    };
    let (width, height) = match (size_pair("size"), number("width"), number("height")) {
        (Some((width, height)), _, _) => (width, height),
        (None, Some(width), Some(height)) => (width, height),
        // 既没 size 也没 width/height（例如只有 x/y 的 sheet 区域）：不猜尺寸。
        _ => return None,
    };
    let to_px = |value: f64| value.round().max(1.0) as u32;
    // 位置两种写法：`x`/`y`，或 `position: [x, y]`。
    let position = match object.get("position") {
        Some(Value::Array(items)) => items
            .first()
            .and_then(Value::as_f64)
            .map(|x| (x, items.get(1).and_then(Value::as_f64).unwrap_or(0.0))),
        _ => None,
    };
    let x = position
        .map(|(x, _)| x)
        .or_else(|| number("x"))
        .map(|value| value.round().max(0.0) as u32)
        .unwrap_or(0);
    let y = position
        .map(|(_, y)| y)
        .or_else(|| number("y"))
        .map(|value| value.round().max(0.0) as u32)
        .unwrap_or(0);
    let scale = number("scale").filter(|scale| *scale > 0.0 && (*scale - 1.0).abs() > 1e-9);
    let tint = object.get("tint").and_then(parse_color);
    Some((filename, x, y, to_px(width), to_px(height), scale, tint))
}

/// `{r,g,b,a}`（0~1 浮点）→ [`Color`]（0~255）。
fn parse_color(value: &Value) -> Option<Color> {
    let object = value.as_object()?;
    let channel = |key: &str| -> u8 {
        object
            .get(key)
            .and_then(Value::as_f64)
            .map(|value| (value.clamp(0.0, 1.0) * 255.0).round() as u8)
            .unwrap_or(255)
    };
    Some(Color(
        channel("r"),
        channel("g"),
        channel("b"),
        channel("a"),
    ))
}

/// 渲染结果（缺文件/解码失败必须分别计数，不能静默少图）。
#[derive(Debug, Default, Clone)]
pub struct UtilityReport {
    /// 尝试渲染的素材数。
    pub total: usize,
    pub written: usize,
    pub missing_source: usize,
    pub decode_failed: usize,
    /// 解析阶段就跳过的（形态不认识）。
    pub unsupported: usize,
    /// 失败样本（`名字: 原因`）。
    pub samples: Vec<String>,
}

/// 把 GUI 素材渲染到 `<out_dir>/utility/<name>.png`。
pub fn render_utility_icons(
    sources: &IconSources,
    sprites: &[UtilitySprite],
    unsupported: usize,
    out_dir: &Path,
) -> Result<UtilityReport, String> {
    let mut report = UtilityReport {
        total: sprites.len(),
        unsupported,
        ..UtilityReport::default()
    };
    for sprite in sprites {
        let source = match sources.decode(&sprite.filename) {
            Ok(source) => source,
            Err(error) => {
                match error {
                    SourceError::Read(_) => report.missing_source += 1,
                    SourceError::Decode(_) => report.decode_failed += 1,
                }
                const MAX_SAMPLES: usize = 10;
                if report.samples.len() < MAX_SAMPLES {
                    report.samples.push(format!("{}: {error}", sprite.name));
                }
                continue;
            }
        };
        let mut image = source.crop(sprite.x, sprite.y, sprite.width, sprite.height);
        if let Some(tint) = sprite.tint {
            crate::render::apply_tint(&mut image, Some(tint));
        }
        if let Some(scale) = sprite.scale {
            image = image.scaled(scale);
        }
        let dir = out_dir.join("utility");
        std::fs::create_dir_all(&dir)
            .map_err(|error| format!("创建 {} 失败: {error}", dir.display()))?;
        let path = dir.join(format!("{}.png", sprite.name));
        std::fs::write(&path, image.encode_png()?)
            .map_err(|error| format!("写 {} 失败: {error}", path.display()))?;
        report.written += 1;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dump_with(sprites: Value) -> Value {
        serde_json::json!({ "utility-sprites": { "default": sprites } })
    }

    #[test]
    fn reads_size_width_and_sheet_regions() {
        let dump = dump_with(serde_json::json!({
            "empty_module_slot": {
                "filename": "__core__/graphics/icons/mip/empty-module-slot.png",
                "priority": "extra-high-no-scale",
                "size": 64,
                "mipmap_count": 2,
                "flags": ["gui-icon"]
            },
            "fuel_icon": {
                "filename": "__core__/graphics/icons/alerts/fuel-icon-red.png",
                "width": 64,
                "height": 64
            },
            "equipment_slot": {
                "filename": "__core__/graphics/gui-new.png",
                "width": 80,
                "height": 80,
                "x": 0,
                "y": 930,
                "scale": 0.5
            }
        }));
        let parsed = parse_utility_sprites(&dump);
        assert!(parsed.skipped.is_empty(), "{:?}", parsed.skipped);
        let by_name = |name: &str| {
            parsed
                .sprites
                .iter()
                .find(|sprite| sprite.name == name)
                .expect("素材存在")
                .clone()
        };
        assert_eq!(by_name("empty_module_slot").width, 64);
        assert_eq!(by_name("empty_module_slot").height, 64);
        assert_eq!(by_name("empty_module_slot").x, 0);
        let fuel = by_name("fuel_icon");
        assert_eq!((fuel.width, fuel.height), (64, 64));
        let slot = by_name("equipment_slot");
        assert_eq!((slot.x, slot.y, slot.width, slot.height), (0, 930, 80, 80));
        assert_eq!(slot.scale, Some(0.5));
    }

    #[test]
    fn unknown_shapes_are_reported_not_silently_dropped() {
        let dump = dump_with(serde_json::json!({
            "weird": { "priority": "medium", "flags": ["icon"] },
            "variations": [{ "filename": "a.png", "size": 32 }, { "filename": "b.png", "size": 32 }]
        }));
        let parsed = parse_utility_sprites(&dump);
        assert_eq!(parsed.sprites.len(), 1, "数组取第一个");
        assert_eq!(parsed.sprites[0].name, "variations");
        assert_eq!(parsed.skipped.len(), 1);
        assert!(
            parsed.skipped[0].starts_with("weird:"),
            "{:?}",
            parsed.skipped
        );
    }

    #[test]
    fn missing_section_is_not_an_error() {
        let parsed = parse_utility_sprites(&serde_json::json!({ "item": {} }));
        assert!(parsed.sprites.is_empty());
        assert!(parsed.skipped.is_empty());
    }
}
