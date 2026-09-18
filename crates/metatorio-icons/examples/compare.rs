//! 验收工具：把自己渲染的图标与游戏官方导出的图标逐像素比对。
//!
//! 用法（在仓库根目录）：
//!
//! ```text
//! cargo run -p metatorio-icons --example compare -- \
//!   --context "C:\Users\<你>\AppData\Roaming\com.mirac.metatorio-app\contexts\2d1e8c21a400155c" \
//!   --game "D:\异星工厂\Factorio_2.1" \
//!   [--mods <mod 目录>] [--type item] [--limit 200] [--tolerance 2] [--dump-only]
//! ```
//!
//! `--context` 目录里应有 `data-raw-dump.json` 与 `icons/`（官方导出结果）。

use std::path::{Path, PathBuf};

use metatorio_icons::compare::{CompareReport, diff_against_official};
use metatorio_icons::image::Rgba8;
use metatorio_icons::render::{RenderOptions, ScaleLaw};
use metatorio_icons::{IconSources, render_prototype_icon};

struct Args {
    context: PathBuf,
    game: PathBuf,
    mods: Option<PathBuf>,
    only_type: Option<String>,
    limit: Option<usize>,
    tolerance: u8,
    dump_only: bool,
    save: Option<PathBuf>,
    /// `--show <type>/<name>`：打印该原型解析出来的层清单 + 与官方图在若干像素上的对照。
    show: Option<String>,
    /// `--pixels x,y;x,y`：配合 `--show` 指定要对照的像素。
    pixels: Vec<(u32, u32)>,
    /// `--fit-scale`：从官方导出反推「层的显式 scale ↔ 实际绘制尺寸」的对应关系。
    fit_scale: bool,
    /// `--sweep`：把所有候选 scale 口径各跑一遍，按像素匹配率挑最好的。
    sweep: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut context = None;
    let mut game = None;
    let mut mods = None;
    let mut only_type = None;
    let mut limit = None;
    let mut tolerance = 2u8;
    let mut dump_only = false;
    let mut save = None;
    let mut show = None;
    let mut pixels = Vec::new();
    let mut fit_scale = false;
    let mut sweep = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut next = |name: &str| -> Result<String, String> {
            args.next().ok_or_else(|| format!("{name} 后面缺少取值"))
        };
        match arg.as_str() {
            "--context" => context = Some(PathBuf::from(next("--context")?)),
            "--game" => game = Some(PathBuf::from(next("--game")?)),
            "--mods" => mods = Some(PathBuf::from(next("--mods")?)),
            "--type" => only_type = Some(next("--type")?),
            "--limit" => {
                limit = Some(
                    next("--limit")?
                        .parse::<usize>()
                        .map_err(|error| format!("--limit 解析失败: {error}"))?,
                )
            }
            "--tolerance" => {
                tolerance = next("--tolerance")?
                    .parse::<u8>()
                    .map_err(|error| format!("--tolerance 解析失败: {error}"))?
            }
            "--dump-only" => dump_only = true,
            "--save" => save = Some(PathBuf::from(next("--save")?)),
            "--show" => show = Some(next("--show")?),
            "--fit-scale" => fit_scale = true,
            "--sweep" => sweep = true,
            "--pixels" => {
                for pair in next("--pixels")?.split(';') {
                    let (x, y) = pair
                        .split_once(',')
                        .ok_or_else(|| format!("--pixels 取值应为 x,y;x,y：{pair}"))?;
                    pixels.push((
                        x.trim().parse::<u32>().map_err(|e| e.to_string())?,
                        y.trim().parse::<u32>().map_err(|e| e.to_string())?,
                    ));
                }
            }
            other => return Err(format!("未知参数: {other}")),
        }
    }
    Ok(Args {
        context: context.ok_or("缺少 --context")?,
        game: game.ok_or("缺少 --game")?,
        mods,
        only_type,
        limit,
        tolerance,
        dump_only,
        save,
        show,
        pixels,
        fit_scale,
        sweep,
    })
}

fn main() -> Result<(), String> {
    let args = parse_args()?;
    let dump_path = args.context.join("data-raw-dump.json");
    let icons_root = args.context.join("icons");
    println!("dump   : {}", dump_path.display());
    println!("icons  : {}", icons_root.display());
    println!("game   : {}", args.game.display());
    println!("mods   : {:?}", args.mods);

    let raw = std::fs::read(&dump_path).map_err(|error| format!("读 dump 失败: {error}"))?;
    let dump: serde_json::Value =
        serde_json::from_slice(&raw).map_err(|error| format!("解析 dump 失败: {error}"))?;
    let store = metatorio_data::store::PrototypeStore::load(&dump)
        .map_err(|error| format!("加载原型仓库失败: {error}"))?;
    println!("原型   : {} 条", store.len());

    let sources = IconSources::from_game_root(&args.game, args.mods.as_deref())?;
    println!("来源   : {}", sources.root_names().join(", "));

    if let Some(target) = &args.show {
        return show_prototype(target, &store, &sources, &icons_root, &args.pixels);
    }
    if args.fit_scale {
        return fit_layer_scale(
            &store,
            &sources,
            &icons_root,
            args.only_type.as_deref(),
            args.limit,
        );
    }
    if args.sweep {
        return sweep_laws(
            &store,
            &sources,
            &icons_root,
            args.only_type.as_deref(),
            args.limit,
            args.tolerance,
            std::env::var("METATORIO_SWEEP_SCALED").is_ok(),
        );
    }

    let mut report = CompareReport::default();
    let mut per_type: std::collections::BTreeMap<String, CompareReport> = Default::default();
    let mut missing_sources = 0usize;
    let mut decoded = 0usize;
    let mut no_icon = 0usize;

    for (_, records) in &store.groups {
        for record in records.values() {
            if let Some(only) = &args.only_type {
                if &record.type_ != only {
                    continue;
                }
            }
            if args.limit.is_some_and(|limit| report.total >= limit) {
                break;
            }
            // 官方只导出「有图标定义」的原型；没有图标定义的不算失败。
            let Some(component) = record.component::<metatorio_data::IconComponent>() else {
                continue;
            };
            if component.icons.is_empty() && component.icon.is_none() {
                continue;
            }
            let reference_path = icons_root
                .join(&record.type_)
                .join(format!("{}.png", record.name));
            if !reference_path.is_file() {
                report.record_missing_reference();
                continue;
            }
            match render_prototype_icon(record, &sources) {
                Ok(ours) => {
                    let bytes = std::fs::read(&reference_path)
                        .map_err(|error| format!("读参考图失败: {error}"))?;
                    let reference = Rgba8::decode_png(&bytes)
                        .map_err(|error| format!("解码参考图失败: {error}"))?;
                    let stats = diff_against_official(&ours, &reference, args.tolerance);
                    if (args.dump_only || args.save.is_some()) && !stats.is_exact() {
                        let dir = args.save.clone().unwrap_or_else(std::env::temp_dir);
                        std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
                        let ours_path =
                            dir.join(format!("{}-{}_ours.png", record.type_, record.name));
                        std::fs::write(&ours_path, ours.encode_png().map_err(|e| e)?)
                            .map_err(|error| error.to_string())?;
                        let ref_path =
                            dir.join(format!("{}-{}_official.png", record.type_, record.name));
                        std::fs::copy(&reference_path, &ref_path)
                            .map_err(|error| error.to_string())?;
                    }
                    report.record(&format!("{}/{}", record.type_, record.name), &stats);
                    per_type
                        .entry(record.type_.clone())
                        .or_default()
                        .record(&record.name, &stats);
                }
                Err(error) => {
                    report.record_render_failure();
                    match error {
                        metatorio_icons::IconRenderError::MissingSource { .. } => {
                            missing_sources += 1
                        }
                        metatorio_icons::IconRenderError::Decode { .. } => decoded += 1,
                        metatorio_icons::IconRenderError::NoIcon => no_icon += 1,
                    }
                    if report.render_failed <= 5 {
                        println!("  渲染失败 {}/{}: {error}", record.type_, record.name);
                    }
                }
            }
        }
    }

    println!("\n=== 总览 ===");
    println!(
        "参与 {}，其中参考图缺失 {}、渲染失败 {}（缺文件 {}、解码 {}、无图标 {}）",
        report.total,
        report.reference_missing,
        report.render_failed,
        missing_sources,
        decoded,
        no_icon
    );
    println!(
        "逐像素完全一致 {}/{}（{:.1}%），平均匹配率 {:.3}%，最大通道差 {}（tolerance {}）",
        report.exact,
        report.compared(),
        if report.compared() == 0 {
            0.0
        } else {
            report.exact as f64 * 100.0 / report.compared() as f64
        },
        report.average_match_ratio() * 100.0,
        report.worst_delta,
        args.tolerance
    );
    println!("\n最差 10 个：");
    for (name, ratio, delta) in &report.worst {
        println!("  {name}: 匹配 {:.2}%，最大差 {delta}", ratio * 100.0);
    }
    println!("\n按类型：");
    for (type_, type_report) in &per_type {
        println!(
            "  {type_}: {} 张，完全一致 {}，平均匹配 {:.2}%",
            type_report.compared(),
            type_report.exact,
            type_report.average_match_ratio() * 100.0
        );
    }
    Ok(())
}

#[allow(dead_code)]
fn unused(_: &Path) {}

/// `--show`：打印某个原型的层清单、每层源图尺寸，以及指定像素上与官方图的对照。
fn show_prototype(
    target: &str,
    store: &metatorio_data::store::PrototypeStore,
    sources: &IconSources,
    icons_root: &Path,
    pixels: &[(u32, u32)],
) -> Result<(), String> {
    let (type_, name) = target
        .split_once('/')
        .ok_or_else(|| format!("--show 取值应为 <type>/<name>：{target}"))?;
    let record = store
        .groups
        .values()
        .flat_map(|records| records.values())
        .find(|record| record.type_ == type_ && record.name == name)
        .ok_or_else(|| format!("找不到原型 {target}"))?;
    let component = record
        .component::<metatorio_data::IconComponent>()
        .ok_or_else(|| format!("{target} 没有 IconComponent"))?;
    println!("\n=== {target} ===");
    println!(
        "原型 icon_size = {:?}，icons 层数 = {}，legacy icon = {:?}",
        component.icon_size,
        component.icons.len(),
        component.icon
    );
    for (index, layer) in component.icons.iter().enumerate() {
        let size = sources
            .read(&layer.icon)
            .ok()
            .and_then(|bytes| Rgba8::decode_png(&bytes).ok())
            .map(|image| format!("{}x{}", image.width, image.height))
            .unwrap_or_else(|| "（读不到）".to_string());
        println!(
            "  层 {index}: {} 源尺寸={size} icon_size={:?} scale={:?} shift={:?} tint={:?} floating={}",
            layer.icon, layer.icon_size, layer.scale, layer.shift, layer.tint, layer.floating
        );
    }
    let ours = render_prototype_icon(record, sources).map_err(|error| error.to_string())?;
    let reference_path = icons_root.join(type_).join(format!("{name}.png"));
    let reference = Rgba8::decode_png(&std::fs::read(&reference_path).map_err(|e| e.to_string())?)
        .map_err(|error| error.to_string())?;
    println!(
        "我们 {}x{}，官方 {}x{}",
        ours.width, ours.height, reference.width, reference.height
    );
    let ours_pre = ours.premultiplied();
    for (x, y) in pixels {
        if *x >= ours.width || *y >= ours.height {
            continue;
        }
        println!(
            "  像素 ({x},{y})：我们(直通)={:?} 预乘后={:?} 官方={:?}",
            ours.pixel(*x, *y),
            ours_pre.pixel(*x, *y),
            reference.pixel(*x, *y)
        );
    }
    // 包围盒诊断：以「只画第 0 层」为基线，与官方/与我们的差异区域就是叠加层的落点，
    // 据此可以反推官方的 scale / shift 口径。
    if component.icons.len() > 1 {
        let expected = component
            .icon_size
            .map(|size| size.max(1) as u32)
            .unwrap_or_else(|| metatorio_icons::render::expected_icon_size(&record.type_));
        let base_only = single_layer_component(component, 0);
        let base_image = metatorio_icons::render::render_icon(&base_only, expected, sources)
            .map_err(|error| error.to_string())?;
        let base_pre = base_image.premultiplied();
        println!("画布边长 = {expected}");
        println!(
            "  官方 vs 只第 0 层 差异包围盒: {:?}",
            diff_bbox(&reference, &base_pre)
        );
        println!(
            "  我们 vs 只第 0 层 差异包围盒: {:?}",
            diff_bbox(&ours_pre, &base_pre)
        );
        for index in 1..component.icons.len() {
            let solo = single_layer_component(component, index);
            if let Ok(image) = metatorio_icons::render::render_icon(&solo, expected, sources) {
                let empty = Rgba8::transparent(image.width, image.height);
                println!(
                    "  只画第 {index} 层 包围盒: {:?}",
                    diff_bbox(&image.premultiplied(), &empty)
                );
            }
        }
    }
    Ok(())
}

/// `--fit-scale`：用官方导出反推「显式 scale ↔ 实际绘制尺寸」。
///
/// 做法：把某个原型「只画第 0 层」当基线，官方图与基线的差异区域就是后续层的足迹；
/// 再看我们自己画同一层（当前模型）的足迹。两者线性尺寸之比 = 官方相对当前模型的比例，
/// 于是可以按 (scale, icon_size, expected) 归类出真实口径。
fn fit_layer_scale(
    store: &metatorio_data::store::PrototypeStore,
    sources: &IconSources,
    icons_root: &Path,
    only_type: Option<&str>,
    limit: Option<usize>,
) -> Result<(), String> {
    use std::collections::BTreeMap;
    let mut buckets: BTreeMap<(String, u32, u32), Vec<f64>> = BTreeMap::new();
    let mut visited = 0usize;
    for records in store.groups.values() {
        for record in records.values() {
            if let Some(only) = only_type {
                if record.type_ != only {
                    continue;
                }
            }
            if limit.is_some_and(|limit| visited >= limit) {
                break;
            }
            let Some(component) = record.component::<metatorio_data::IconComponent>() else {
                continue;
            };
            // 只看「第 1 层有显式 scale」的原型：其余层的口径已知（=1）。
            let Some(layer) = component.icons.get(1) else {
                continue;
            };
            let Some(explicit) = layer.scale else {
                continue;
            };
            let reference_path = icons_root
                .join(&record.type_)
                .join(format!("{}.png", record.name));
            if !reference_path.is_file() {
                continue;
            }
            let Ok(reference) =
                Rgba8::decode_png(&std::fs::read(&reference_path).unwrap_or_default())
            else {
                continue;
            };
            let expected = component
                .icon_size
                .map(|size| size.max(1) as u32)
                .unwrap_or_else(|| metatorio_icons::render::expected_icon_size(&record.type_));
            let base = single_layer_component(component, 0);
            let Ok(base_image) = metatorio_icons::render::render_icon(&base, expected, sources)
            else {
                continue;
            };
            let base_pre = base_image.premultiplied();
            let Some(official_box) = diff_bbox(&reference, &base_pre) else {
                continue;
            };
            let solo = single_layer_component(component, 1);
            let Ok(solo_image) = metatorio_icons::render::render_icon(&solo, expected, sources)
            else {
                continue;
            };
            let Some(mine_box) = diff_bbox(
                &solo_image.premultiplied(),
                &Rgba8::transparent(solo_image.width, solo_image.height),
            ) else {
                continue;
            };
            let span = |b: (u32, u32, u32, u32)| (b.2 - b.0 + 1).max(b.3 - b.1 + 1) as f64;
            let ratio = span(official_box) / span(mine_box);
            let icon_size = layer.icon_size.map(|size| size as u32).unwrap_or(expected);
            buckets
                .entry((format!("{explicit}"), icon_size, expected))
                .or_default()
                .push(ratio);
            visited += 1;
        }
    }
    println!("\n=== 显式 scale 的实际比例（官方足迹 / 我们的足迹）===");
    println!("显式 scale | icon_size | expected | 样本 | 官方比例中位数");
    for ((scale, icon_size, expected), ratios) in &buckets {
        let mut sorted = ratios.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let median = sorted[sorted.len() / 2];
        println!(
            "  {scale:>8} | {icon_size:>9} | {expected:>8} | {:>4} | {median:.3}",
            ratios.len()
        );
    }
    Ok(())
}

/// `--sweep`：把候选 scale 口径各跑一遍，按像素匹配率挑最好的。
fn sweep_laws(
    store: &metatorio_data::store::PrototypeStore,
    sources: &IconSources,
    icons_root: &Path,
    only_type: Option<&str>,
    limit: Option<usize>,
    tolerance: u8,
    sweep_scale_only: bool,
) -> Result<(), String> {
    let mut results: Vec<(ScaleLaw, f64, CompareReport)> = Vec::new();
    for law in ScaleLaw::all() {
        for shift in [2.0f64, 1.0] {
            let options = RenderOptions {
                scale_law: law,
                shift_pixels_per_unit: shift,
            };
            let mut report = CompareReport::default();
            let mut visited = 0usize;
            'outer: for records in store.groups.values() {
                for record in records.values() {
                    if let Some(only) = only_type {
                        if record.type_ != only {
                            continue;
                        }
                    }
                    if limit.is_some_and(|limit| visited >= limit) {
                        break 'outer;
                    }
                    let Some(component) = record.component::<metatorio_data::IconComponent>()
                    else {
                        continue;
                    };
                    if component.icons.is_empty() && component.icon.is_none() {
                        continue;
                    }
                    // 可选：只统计「有层显式给了 scale」的原型——scale 口径唯一有争议的子集。
                    if sweep_scale_only
                        && !component.icons.iter().any(|layer| layer.scale.is_some())
                    {
                        continue;
                    }
                    let reference_path = icons_root
                        .join(&record.type_)
                        .join(format!("{}.png", record.name));
                    if !reference_path.is_file() {
                        continue;
                    }
                    visited += 1;
                    let Ok(ours) = metatorio_icons::render::render_prototype_icon_with(
                        record, sources, options,
                    ) else {
                        report.record_render_failure();
                        continue;
                    };
                    let Ok(reference) =
                        Rgba8::decode_png(&std::fs::read(&reference_path).unwrap_or_default())
                    else {
                        report.record_missing_reference();
                        continue;
                    };
                    let stats = diff_against_official(&ours, &reference, tolerance);
                    report.record(&format!("{}/{}", record.type_, record.name), &stats);
                }
            }
            results.push((law, shift, report));
        }
    }
    println!("\n=== scale 口径定标（匹配率越高越好）===");
    for (law, shift, report) in &results {
        println!(
            "  {:<36} shift×{shift}: 平均 {:>6.2}%  完好 {}/{}",
            law.name(),
            report.average_match_ratio() * 100.0,
            report.exact,
            report.compared()
        );
    }
    if let Some((law, shift, report)) = results.iter().max_by(|a, b| {
        a.2.average_match_ratio()
            .partial_cmp(&b.2.average_match_ratio())
            .unwrap()
    }) {
        println!(
            "\n最佳：{}（shift×{shift}），平均匹配 {:.2}%",
            law.name(),
            report.average_match_ratio() * 100.0
        );
    }
    Ok(())
}

/// 只保留第 `index` 层的副本（诊断用）。
fn single_layer_component(
    component: &metatorio_data::IconComponent,
    index: usize,
) -> metatorio_data::IconComponent {
    let mut clone = component.clone();
    clone.icon = None;
    clone.icons = vec![component.icons[index].clone()];
    clone
}

/// 两张图的差异包围盒（阈值 8）；没有差异返回 None。
fn diff_bbox(a: &Rgba8, b: &Rgba8) -> Option<(u32, u32, u32, u32)> {
    let width = a.width.min(b.width);
    let height = a.height.min(b.height);
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0u32, 0u32);
    for y in 0..height {
        for x in 0..width {
            let pa = a.pixel(x, y);
            let pb = b.pixel(x, y);
            let delta = (0..4)
                .map(|index| pa[index].abs_diff(pb[index]))
                .max()
                .unwrap_or(0);
            if delta > 8 {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    if x0 == u32::MAX {
        None
    } else {
        Some((x0, y0, x1, y1))
    }
}
