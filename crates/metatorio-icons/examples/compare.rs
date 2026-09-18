//! 验收工具：把自己渲染的图标与游戏官方导出的图标逐像素比对。
//!
//! 用法（在仓库根目录）：
//!
//! ```text
//! cargo run -p metatorio-icons --example compare -- \
//!   --context "C:\Users\<你>\AppData\Roaming\com.mirac.metatorio-app\contexts\2d1e8c21a400155c" \
//!   --game "D:\异星工厂\Factorio_2.1" \
//!   [--mods <mod 目录>] [--type item] [--limit 200] [--tolerance 2] [--types] [--dump-only]
//! ```
//!
//! `--context` 目录里应有 `data-raw-dump.json` 与 `icons/`（官方导出结果）。
//!
//! 输出分四块，**四块都算数**：参与比对的（按参考图目录 + 可选按原型类型）、官方有图但
//! dump 里没有图标定义（游戏自动生成，未实现）、dump 有定义但官方没导出图、以及官方有图
//! 但原型仓库里没有这条原型（类型不在关注列表）——跳过的东西必须被数出来。

use std::path::{Path, PathBuf};

use metatorio_icons::compare::{CompareReport, diff_against_official};
use metatorio_icons::image::Rgba8;
use metatorio_icons::render::{RenderOptions, ScaleLaw};
use metatorio_icons::{IconSources, render_prototype_icon};

/// 官方导出的参考图索引。
///
/// **导出目录名不一定等于原型 `type`**（实测）：所有实体类型（`assembling-machine`、
/// `tree`、`explosion`、`corpse`、`simple-entity`、`resource` …）的图标都写在
/// `entity/` 下，所有物品子类型（`ammo`、`gun`、`module`、`armor`、`capsule`、
/// `item-with-entity-data` …）都写在 `item/` 下，其余类型才是 `<type>/`。
/// 所以按**文件名**建索引，解析时优先取与原型 `type` 同名的目录，取不到再用唯一候选；
/// 并把「实际用到的目录」记下来——这条规律要靠实测报告，不能靠猜。
struct ReferenceIndex {
    by_name: std::collections::BTreeMap<String, Vec<(String, PathBuf)>>,
    used: std::collections::BTreeSet<(String, String)>,
    folders: std::collections::BTreeSet<String>,
    total: usize,
}

impl ReferenceIndex {
    fn scan(root: &Path) -> Result<Self, String> {
        let mut by_name: std::collections::BTreeMap<String, Vec<(String, PathBuf)>> =
            Default::default();
        let mut folders = std::collections::BTreeSet::new();
        let mut total = 0usize;
        let entries = std::fs::read_dir(root)
            .map_err(|error| format!("读参考图目录 {} 失败: {error}", root.display()))?;
        for entry in entries.flatten() {
            let folder = entry.path();
            if !folder.is_dir() {
                continue;
            }
            let folder_name = entry.file_name().to_string_lossy().into_owned();
            folders.insert(folder_name.clone());
            for file in std::fs::read_dir(&folder)
                .map_err(|e| e.to_string())?
                .flatten()
            {
                let path = file.path();
                if path.extension().is_none_or(|ext| ext != "png") {
                    continue;
                }
                let Some(stem) = path
                    .file_stem()
                    .map(|stem| stem.to_string_lossy().into_owned())
                else {
                    continue;
                };
                total += 1;
                by_name
                    .entry(stem)
                    .or_default()
                    .push((folder_name.clone(), path));
            }
        }
        Ok(Self {
            by_name,
            used: Default::default(),
            folders,
            total,
        })
    }

    /// 解析某个原型的参考图：优先 `preferred`（该原型实际会落到的目录），否则取候选里
    /// 字典序最小的，保证结果稳定可复现。
    fn resolve(&self, preferred: &str, name: &str) -> Option<(String, PathBuf)> {
        let candidates = self.by_name.get(name)?;
        let same_folder = candidates
            .iter()
            .find(|(folder, _)| folder == preferred)
            .cloned();
        same_folder.or_else(|| {
            candidates
                .iter()
                .min_by(|left, right| left.0.cmp(&right.0))
                .cloned()
        })
    }

    fn mark_used(&mut self, folder: &str, name: &str) {
        self.used.insert((folder.to_string(), name.to_string()));
    }

    /// 没用上的参考图，分两种如实报告：
    /// `absent` = 原型仓库里压根没有这个文件名对应的原型（类型不在关注列表）；
    /// `duplicate` = 原型在，只是同名图有多份，只用了「原型 type 优先」的那一份。
    fn unused(
        &self,
        store_names: &std::collections::BTreeSet<String>,
    ) -> (
        std::collections::BTreeMap<String, usize>,
        std::collections::BTreeMap<String, usize>,
    ) {
        let mut absent: std::collections::BTreeMap<String, usize> = Default::default();
        let mut duplicate: std::collections::BTreeMap<String, usize> = Default::default();
        for (name, candidates) in &self.by_name {
            for (folder, _) in candidates {
                if self.used.contains(&(folder.clone(), name.clone())) {
                    continue;
                }
                let bucket = if store_names.contains(name) {
                    &mut duplicate
                } else {
                    &mut absent
                };
                *bucket.entry(folder.clone()).or_default() += 1;
            }
        }
        (absent, duplicate)
    }
}

/// 该原型的参考图应该落在哪个目录。
///
/// 官方导出按「GUI 归类」分目录，而不是按原型 `type`：**物品子类型**（`ammo`、`gun`、
/// `module`、`armor`、`capsule` …）都进 `item/`，**实体类型**都进 `entity/`，其余用自己的
/// `type`。只看名字会踩同名：`module/fish` 会拿到 `entity/fish.png`（实测差 2.86%），
/// 而它对应的其实是 `item/fish.png`。
fn reference_folder(record: &metatorio_data::store::PrototypeRecord) -> String {
    use metatorio_data::store::PrototypeGroup;
    match record.group {
        PrototypeGroup::Item => "item".to_string(),
        PrototypeGroup::Entity => "entity".to_string(),
        _ => record.type_.clone(),
    }
}

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
    /// `--types`：额外打印「按原型类型」的完整统计（默认只按参考图目录汇总，输出有界）。
    types: bool,
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
    let mut types = false;
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
            "--types" => types = true,
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
        types,
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

    let mut references = ReferenceIndex::scan(&icons_root)?;
    println!(
        "参考图 : {} 张，{} 个目录",
        references.total,
        references.folders.len()
    );
    // 原型仓库里的全部名字（含未参与比对的类型），用来区分「真没这条原型」与「同名多份」。
    let store_names: std::collections::BTreeSet<String> = store
        .groups
        .values()
        .flat_map(|records| records.values())
        .map(|record| record.name.clone())
        .collect();

    if let Some(target) = &args.show {
        return show_prototype(target, &store, &sources, &references, &args.pixels);
    }
    if args.fit_scale {
        return fit_layer_scale(
            &store,
            &sources,
            &references,
            args.only_type.as_deref(),
            args.limit,
        );
    }
    if args.sweep {
        return sweep_laws(
            &store,
            &sources,
            &references,
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
    let mut derived_missing = 0usize;
    // 没有图标定义、官方也没导出图：一致，没什么可画的（单独数出来，避免被当成漏掉）。
    let mut skipped_without_icon = 0usize;
    // 官方有图、dump 里没有图标定义——**现在按官方文档的推导规则画**，单独统计匹配率。
    let mut derived: std::collections::BTreeMap<String, CompareReport> = Default::default();
    // 反方向：dump 里有图标定义、官方导出里却没有图。这些也没参与比对。
    let mut no_reference: std::collections::BTreeMap<String, usize> = Default::default();
    // 实测的「原型 type → 实际目录」映射（用来确认导出规律，而不是假设它）。
    let mut folder_by_type: std::collections::BTreeMap<(String, String), usize> =
        Default::default();
    // 按**参考图目录**汇总（实体 / 物品子类型 / 配方 …），比按原型 type 汇总更能说明覆盖面。
    let mut per_folder: std::collections::BTreeMap<String, CompareReport> = Default::default();

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
            let defined = metatorio_icons::has_icon_definition(record);
            let Some((folder, reference_path)) =
                references.resolve(&reference_folder(record), &record.name)
            else {
                if defined {
                    report.record_missing_reference();
                    *no_reference.entry(record.type_.clone()).or_default() += 1;
                } else {
                    skipped_without_icon += 1;
                }
                continue;
            };
            *folder_by_type
                .entry((record.type_.clone(), folder.clone()))
                .or_default() += 1;
            references.mark_used(&folder, &record.name);
            match metatorio_icons::render_record_icon_with(
                &store,
                record,
                &sources,
                RenderOptions::default(),
            ) {
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
                    per_folder
                        .entry(folder.clone())
                        .or_default()
                        .record(&record.name, &stats);
                    if !defined {
                        derived
                            .entry(record.type_.clone())
                            .or_default()
                            .record(&record.name, &stats);
                    }
                }
                Err(error) => {
                    report.record_render_failure();
                    match error {
                        metatorio_icons::IconRenderError::MissingSource { .. } => {
                            missing_sources += 1
                        }
                        metatorio_icons::IconRenderError::Decode { .. } => decoded += 1,
                        metatorio_icons::IconRenderError::NoIcon { .. } => no_icon += 1,
                        metatorio_icons::IconRenderError::DerivedMissing { .. } => {
                            derived_missing += 1
                        }
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
        "参与 {}，其中参考图缺失 {}、渲染失败 {}（缺文件 {}、解码 {}、无图标 {}、推导失败 {}）；\
         无图标定义且官方也没图（一致，跳过）{}",
        report.total,
        report.reference_missing,
        report.render_failed,
        missing_sources,
        decoded,
        no_icon,
        derived_missing,
        skipped_without_icon
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
    println!("\n按参考图目录：");
    for (folder, folder_report) in &per_folder {
        println!(
            "  {folder}: {} 张，完全一致 {}，平均匹配 {:.2}%",
            folder_report.compared(),
            folder_report.exact,
            folder_report.average_match_ratio() * 100.0
        );
    }
    if args.types {
        println!("\n按原型类型：");
        for (type_, type_report) in &per_type {
            println!(
                "  {type_}: {} 张，完全一致 {}，平均匹配 {:.2}%",
                type_report.compared(),
                type_report.exact,
                type_report.average_match_ratio() * 100.0
            );
        }
    }
    if !derived.is_empty() {
        let total: usize = derived.values().map(CompareReport::compared).sum();
        let ratio_sum: f64 = derived
            .values()
            .map(|report| report.average_match_ratio() * report.compared() as f64)
            .sum();
        println!(
            "\n=== 按官方推导规则画的（dump 里没有图标定义，{total} 张，平均匹配 {:.2}%）===",
            if total == 0 {
                0.0
            } else {
                ratio_sum * 100.0 / total as f64
            }
        );
        for (type_, type_report) in &derived {
            println!(
                "  {type_}: {} 张，完全一致 {}，平均匹配 {:.2}%",
                type_report.compared(),
                type_report.exact,
                type_report.average_match_ratio() * 100.0
            );
        }
    }
    if !no_reference.is_empty() {
        println!("\n=== dump 有图标定义、官方导出里没有图（未参与比对）===");
        for (type_, count) in &no_reference {
            println!("  {type_}: {count} 张");
        }
    }
    println!("\n=== 实测的「原型 type → 参考图目录」（只为确认导出规律）===");
    let mut folder_rows: Vec<(&str, &str, usize)> = folder_by_type
        .iter()
        .map(|((type_, folder), count)| (type_.as_str(), folder.as_str(), *count))
        .collect();
    folder_rows.sort_by_key(|row| std::cmp::Reverse(row.2));
    for (type_, folder, count) in folder_rows {
        if folder == type_ {
            continue;
        }
        println!("  {type_} → {folder}: {count} 张");
    }
    let unused = references.unused(&store_names);
    for (title, bucket) in [
        (
            "官方导出里有图、但原型仓库里没有这条原型（未参与比对）",
            unused.0,
        ),
        (
            "官方导出里同名图有多份、只用了「原型 type 优先」的那份",
            unused.1,
        ),
    ] {
        if bucket.is_empty() {
            continue;
        }
        let total: usize = bucket.values().sum();
        println!("\n=== {title}（共 {total} 张）===");
        for (folder, count) in &bucket {
            println!("  {folder}: {count} 张");
        }
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
    references: &ReferenceIndex,
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
    let preferred = reference_folder(record);
    let (folder, reference_path) = references
        .resolve(&preferred, name)
        .ok_or_else(|| format!("官方导出里找不到 {target} 的参考图"))?;
    let reference = Rgba8::decode_png(&std::fs::read(&reference_path).map_err(|e| e.to_string())?)
        .map_err(|error| error.to_string())?;
    if folder != preferred {
        println!("参考图目录：{folder}（{target} 应该落在 {preferred}/）");
    }
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
    references: &ReferenceIndex,
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
            let Some((_, reference_path)) =
                references.resolve(&reference_folder(record), &record.name)
            else {
                continue;
            };
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
    references: &ReferenceIndex,
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
                    let Some((_, reference_path)) =
                        references.resolve(&reference_folder(record), &record.name)
                    else {
                        continue;
                    };
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
