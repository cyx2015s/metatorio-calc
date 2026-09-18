//! 图标文件来源解析：`__base__` / `__core__` / `__mod__` → 真实字节。
//!
//! Factorio 的图标路径长这样：
//! - `__base__/graphics/icons/iron-plate.png` → `<游戏>/data/base/graphics/…`
//! - `__core__/graphics/cancel.png` → `<游戏>/data/core/graphics/…`
//! - `__aai-industry__/graphics/icons/x.png` → mod 目录里的 `aai-industry_0.7.3.zip`
//!   内条目 `aai-industry_0.7.3/graphics/icons/x.png`（或解压出来的同名目录）
//!
//! 因此解析规则是把 `__名字__` 前缀映射到一个**根**（目录或 zip），其余部分作为
//! 相对路径。zip 只在真正读条目时才打开，不预先解压整包。

use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::image::Rgba8;

/// 一个资源根：目录，或一个 mod 的 zip。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Archive {
    /// 普通目录（`__base__` / `__core__` / 解压出来的 mod）。
    Dir(PathBuf),
    /// mod 的 zip；条目都带 `<modname>_<version>/` 前缀。
    Zip { path: PathBuf, prefix: String },
}

/// 读/解码某个资源时的失败原因（分开报，才能老实统计「缺文件」和「解码失败」）。
#[derive(Debug, Clone)]
pub enum SourceError {
    /// 路径解析不了、文件/条目不存在、读失败。
    Read(String),
    /// 读到了但 PNG 解不开。
    Decode(String),
}

impl std::fmt::Display for SourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Read(error) => write!(f, "{error}"),
            Self::Decode(error) => write!(f, "解码失败: {error}"),
        }
    }
}

impl std::error::Error for SourceError {}

/// 已解码图片的缓存：同一个贴图会被成百上千个原型引用（配方尤其明显），
/// 重复解码是纯浪费。按总字节数封顶，超了整批清空（简单、可预期）。
#[derive(Default)]
struct DecodedCache {
    entries: HashMap<String, Arc<Rgba8>>,
    bytes: usize,
}

/// 解码缓存上限（py 那种大包全量渲染时，这是内存换时间的旋钮）。
const DECODED_CACHE_MAX_BYTES: usize = 128 * 1024 * 1024;

/// 图标（以及其它原型资源）的来源集合。
///
/// 内部带两层缓存（都不改变语义，只省时间）：
/// - **zip 只开一次**：py 的 graphics 包有上百 MB，每次读一张图都重开一遍要好几毫秒；
/// - **解码结果按资源路径缓存**：一张 `recycling.png` 会被几千个配方引用。
pub struct IconSources {
    roots: HashMap<String, Archive>,
    zips: Mutex<HashMap<PathBuf, zip::ZipArchive<File>>>,
    decoded: Mutex<DecodedCache>,
}

impl std::fmt::Debug for IconSources {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IconSources")
            .field("roots", &self.roots.len())
            .finish()
    }
}

impl Default for IconSources {
    fn default() -> Self {
        Self {
            roots: HashMap::new(),
            zips: Mutex::new(HashMap::new()),
            decoded: Mutex::new(DecodedCache::default()),
        }
    }
}

impl IconSources {
    /// 从游戏可执行文件路径推断：`<游戏>/bin/x64/factorio.exe` → `<游戏>/data/base`、
    /// `<游戏>/data/core`，mod 目录默认 `<游戏>/mods`（`mod_dir` 给定时用它）。
    pub fn from_game_exe(exe: &Path, mod_dir: Option<&Path>) -> Result<Self, String> {
        let game = game_root_from_exe(exe)
            .ok_or_else(|| format!("无法从可执行文件路径推断游戏根目录: {}", exe.display()))?;
        Self::from_game_root(&game, mod_dir)
    }

    /// `<游戏>/data/*`（base、core、以及 DLC 的 `space-age` 等）＋**调用方指定的** mod 目录。
    ///
    /// `mod_dir = None` 表示**这个上下文不加载 mod**（导出时留空 mod 目录就是这个意思：
    /// 我们给游戏写了自己的 `config.ini`，游戏只会读原版 + DLC 内容），此时**不去猜**
    /// `<游戏>/mods`——那是「加载了 mod」的情形，语义不同。
    ///
    /// `data/` 下的**每个子目录**都注册成一个来源：图标里写的是 `__space-age__/…`
    /// 这种形式，与 base/core 同级。
    pub fn from_game_root(game: &Path, mod_dir: Option<&Path>) -> Result<Self, String> {
        let data = game.join("data");
        let base = data.join("base");
        if !base.is_dir() {
            return Err(format!("找不到游戏数据目录: {}", base.display()));
        }
        let mut sources = Self::default();
        sources.roots.insert("base".to_string(), Archive::Dir(base));
        if let Ok(entries) = std::fs::read_dir(&data) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let name = dir_name(&path);
                if name == "base" {
                    continue;
                }
                sources.roots.entry(name).or_insert(Archive::Dir(path));
            }
        }
        if let Some(mods) = mod_dir {
            for file in loaded_mods(mods)? {
                sources.roots.insert(file.name, file.archive);
            }
        }
        Ok(sources)
    }

    /// 只有目录（不做 exe 推断）——测试与「只有一个 dump 目录」的场景用。
    pub fn from_roots(roots: impl IntoIterator<Item = (String, Archive)>) -> Self {
        Self {
            roots: roots.into_iter().collect(),
            ..Self::default()
        }
    }

    /// 已解析到的根名字（`base` / `core` / mod 名），便于排查。
    pub fn root_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.roots.keys().cloned().collect();
        names.sort();
        names
    }

    /// 该路径能不能解析到（用于统计「缺多少图标文件」）。
    pub fn exists(&self, spec: &str) -> bool {
        self.read(spec).is_ok()
    }

    /// 读一个原型资源路径（`__base__/graphics/icons/iron-plate.png`）。
    pub fn read(&self, spec: &str) -> Result<Vec<u8>, String> {
        let (root, relative) =
            split_spec(spec).ok_or_else(|| format!("不是 __mod__/ 形式的资源路径: {spec}"))?;
        if relative.is_empty() {
            return Err(format!("资源路径归一化后为空: {spec}"));
        }
        let archive = self.roots.get(root).ok_or_else(|| {
            let mut names = self.root_names();
            let total = names.len();
            names.truncate(6);
            format!(
                "没有这个来源: __{root}__（已知 {total} 个：{}…）",
                names.join(", ")
            )
        })?;
        match archive {
            Archive::Dir(dir) => {
                let path = dir.join(&relative);
                std::fs::read(&path).map_err(|error| format!("读 {} 失败: {error}", path.display()))
            }
            Archive::Zip { path, prefix } => {
                let entry_name = format!("{prefix}{relative}");
                let mut zips = self.zips.lock().map_err(|_| "zip 缓存锁损坏".to_string())?;
                if !zips.contains_key(path) {
                    let file = File::open(path)
                        .map_err(|error| format!("打开 {} 失败: {error}", path.display()))?;
                    let archive = zip::ZipArchive::new(file)
                        .map_err(|error| format!("解析 {} 失败: {error}", path.display()))?;
                    zips.insert(path.clone(), archive);
                }
                let archive = zips.get_mut(path).expect("上面刚插入");
                let mut entry = archive
                    .by_name(&entry_name)
                    .map_err(|_| format!("{} 里没有条目 {entry_name}", path.display()))?;
                let mut bytes = Vec::with_capacity(entry.size() as usize);
                entry
                    .read_to_end(&mut bytes)
                    .map_err(|error| format!("读条目 {entry_name} 失败: {error}"))?;
                Ok(bytes)
            }
        }
    }

    /// 读并**解码**一个资源（带解码缓存；同一个路径只解一次）。
    pub fn decode(&self, spec: &str) -> Result<Arc<Rgba8>, SourceError> {
        if let Ok(cache) = self.decoded.lock()
            && let Some(image) = cache.entries.get(spec)
        {
            return Ok(image.clone());
        }
        let bytes = self.read(spec).map_err(SourceError::Read)?;
        let image = Arc::new(Rgba8::decode_png(&bytes).map_err(SourceError::Decode)?);
        if let Ok(mut cache) = self.decoded.lock() {
            let size = image.pixels.len();
            if cache.bytes + size > DECODED_CACHE_MAX_BYTES && !cache.entries.is_empty() {
                cache.entries.clear();
                cache.bytes = 0;
            }
            cache.bytes += size;
            cache.entries.insert(spec.to_string(), image.clone());
        }
        Ok(image)
    }
}

/// `__base__/graphics/x.png` → `("base", "graphics/x.png")`。
///
/// 相对路径会做**斜杠归一化**：连续 `/` 折成一个。游戏自己的路径解析就能吃
/// `__pyalienlifegraphics__/graphics/icons//x.png`（mod 数据里真有这种双斜杠），
/// 我们不归一化的话 zip 里按条目名查就会失败——`Archive::Zip` 是按名字精确匹配的。
pub fn split_spec(spec: &str) -> Option<(&str, String)> {
    let rest = spec.strip_prefix("__")?;
    let end = rest.find("__")?;
    let (root, tail) = rest.split_at(end);
    let relative = tail.strip_prefix("__")?.strip_prefix('/')?;
    Some((root, normalize_slashes(relative)))
}

/// 把连续 `/` 折成一个（同时也去掉开头多余的 `/`），其余字符原样保留。
fn normalize_slashes(relative: &str) -> String {
    let mut normalized = String::with_capacity(relative.len());
    for segment in relative.split('/') {
        if segment.is_empty() {
            continue;
        }
        if !normalized.is_empty() {
            normalized.push('/');
        }
        normalized.push_str(segment);
    }
    normalized
}

/// `<游戏>/bin/x64/factorio.exe` → `<游戏>`（往上找到含 `data` 的那一层）。
pub fn game_root_from_exe(exe: &Path) -> Option<PathBuf> {
    let mut dir = exe.parent()?;
    for _ in 0..3 {
        if dir.join("data").is_dir() {
            return Some(dir.to_path_buf());
        }
        dir = dir.parent()?;
    }
    None
}

/// 扫到并**会被游戏加载**的一个 mod：名字、版本、资源根。
#[derive(Debug, Clone)]
pub struct LoadedMod {
    pub name: String,
    pub version: Option<String>,
    pub archive: Archive,
}

/// `mod-list.json` 里的一条（只关心启用状态与可选锁定版本）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModListEntry {
    pub name: String,
    pub enabled: bool,
    /// 锁定版本：`mod-list.json` 里给了 `version` 才有（实测这台机器上一条都没有）。
    pub version: Option<String>,
}

/// 读 `<mod 目录>/mod-list.json`。
///
/// 文件不存在 / 解析失败 → 空表：**不知道哪些启用**，调用方要按「不知道」处理，不要猜。
pub fn read_mod_list(dir: &Path) -> Vec<ModListEntry> {
    let Ok(raw) = std::fs::read_to_string(dir.join("mod-list.json")) else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return Vec::new();
    };
    value
        .get("mods")
        .and_then(serde_json::Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| {
                    Some(ModListEntry {
                        name: entry.get("name")?.as_str()?.to_string(),
                        enabled: entry
                            .get("enabled")
                            .and_then(serde_json::Value::as_bool)
                            .unwrap_or(false),
                        version: entry
                            .get("version")
                            .and_then(serde_json::Value::as_str)
                            .map(str::to_string),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// 启用名单（含 `base`）。
pub fn enabled_mod_names(dir: &Path) -> Vec<String> {
    read_mod_list(dir)
        .into_iter()
        .filter(|entry| entry.enabled)
        .map(|entry| entry.name)
        .collect()
}

/// 同名多版本之间的一个候选。
struct ModCandidate {
    name: String,
    version: Option<String>,
    archive: Archive,
}

/// **游戏实际会加载的 mod**，按名字排序。
///
/// 遵守 Factorio 的 mod 目录约定（用户给的规则 + 实测）：
/// - **目录形态**：目录名必须正好是 mod 的 id（**带版本号的目录不接受**），`info.json`
///   直接躺在目录下（不像 zip 会嵌一层 `<名字>_<版本>/`）；版本取 `info.json.version`，
///   没有 `info.json` 的不算 mod。实测这台机器的 3 个目录 mod 都是这个形态。
/// - **zip 形态**：**文件名必须带版本号**（`<名字>_<版本>.zip`），包内是 `<名字>_<版本>/…`；
///   文件名没有版本号的不收（`.modpack.zip` 这类也在这个判据下被挡掉）。
/// - **同名多个候选**：`mod-list.json` 里锁定了版本就用那个版本（同版本时**目录形态优先**）；
///   没锁定就用**能查到的最新版本**（按版本号比较，不是字符串比较），无论目录还是 zip。
///   实测这台机器上 `tanvec-ai-cn` 装了 5 个版本、`ForGavin` 4 个、`tanvec-tweaks` 3 个——
///   以前谁生效取决于 `read_dir` 的顺序，现在确定：取最新。
pub fn loaded_mods(dir: &Path) -> Result<Vec<LoadedMod>, String> {
    let pinned: std::collections::HashMap<String, String> = read_mod_list(dir)
        .into_iter()
        .filter_map(|entry| entry.version.map(|version| (entry.name, version)))
        .collect();
    let mut candidates: std::collections::HashMap<String, Vec<ModCandidate>> = Default::default();
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        // 没有 mod 目录是正常的（纯原版）。
        Err(_) => return Ok(Vec::new()),
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let candidate = if path.is_dir() {
            directory_candidate(&path)
        } else if path.extension().is_some_and(|ext| ext == "zip") {
            zip_candidate(&path)
        } else {
            None
        };
        if let Some(candidate) = candidate {
            candidates
                .entry(candidate.name.clone())
                .or_default()
                .push(candidate);
        }
    }
    let mut loaded: Vec<LoadedMod> = candidates
        .into_iter()
        .filter_map(|(name, mut group)| {
            let pin = pinned.get(&name).map(String::as_str);
            // 锁定版本优先 → 版本号大的优先 → 同版本取目录形态；排序后取最后一个。
            group.sort_by(|left, right| {
                let pinned_rank = |candidate: &ModCandidate| {
                    u8::from(pin.is_some() && candidate.version.as_deref() == pin)
                };
                pinned_rank(left)
                    .cmp(&pinned_rank(right))
                    .then_with(|| {
                        compare_versions(left.version.as_deref(), right.version.as_deref())
                    })
                    .then_with(|| form_rank(&left.archive).cmp(&form_rank(&right.archive)))
            });
            group.pop().map(|winner| LoadedMod {
                name,
                version: winner.version,
                archive: winner.archive,
            })
        })
        .collect();
    loaded.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(loaded)
}

/// 目录形态的候选：目录名就是 id（带版本号的目录**不收**），`info.json` 直接在目录下。
fn directory_candidate(path: &Path) -> Option<ModCandidate> {
    let folder = dir_name(path);
    if folder.is_empty() || version_suffix(&folder).is_some() {
        return None;
    }
    let info = info_json(path)?;
    let name = info.0?;
    Some(ModCandidate {
        name,
        version: info.1,
        archive: Archive::Dir(path.to_path_buf()),
    })
}

/// zip 形态的候选：文件名必须带版本号，包内条目要带 `<前缀>/`。
fn zip_candidate(path: &Path) -> Option<ModCandidate> {
    let stem = path.file_stem()?.to_str()?;
    version_suffix(stem)?;
    let (name, prefix, version) = zip_mod_info(path)?;
    Some(ModCandidate {
        name,
        version,
        archive: Archive::Zip {
            path: path.to_path_buf(),
            prefix,
        },
    })
}

/// 目录形态优先（同名同版本时）。
fn form_rank(archive: &Archive) -> u8 {
    match archive {
        Archive::Dir(_) => 1,
        Archive::Zip { .. } => 0,
    }
}

/// 版本号比较：点分数字逐段比；没有版本的一律排最后。
fn compare_versions(left: Option<&str>, right: Option<&str>) -> std::cmp::Ordering {
    let key = |version: Option<&str>| -> Option<Vec<u64>> {
        let version = version?;
        Some(
            version
                .split('.')
                .filter_map(|part| part.parse::<u64>().ok())
                .collect(),
        )
    };
    match (key(left), key(right)) {
        (None, None) => std::cmp::Ordering::Equal,
        (None, Some(_)) => std::cmp::Ordering::Less,
        (Some(_), None) => std::cmp::Ordering::Greater,
        (Some(left), Some(right)) => left.cmp(&right),
    }
}

/// `<名字>_<版本>` 里的版本段——只有**长得像版本号**才认（全是数字、点分、至少两段）。
///
/// 这样 `some_mod.zip`（没有版本段的包名）不会被当成版本 `mod`，
/// `NoEmptyBarrels_3`（名字里带下划线+数字）也不会被误拆（实测这台机器上真有这个 mod）。
fn version_suffix(stem: &str) -> Option<&str> {
    let (_, version) = stem.rsplit_once('_')?;
    let looks_like_version = version.contains('.')
        && version
            .split('.')
            .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()));
    looks_like_version.then_some(version)
}

/// `<名字>_<版本>` 里的名字段（zip 前缀用；目录名不带版本，不走这条）。
fn name_without_version(stem: &str) -> String {
    match version_suffix(stem) {
        Some(_) => stem
            .rsplit_once('_')
            .map(|(name, _)| name)
            .unwrap_or(stem)
            .to_string(),
        None => stem.to_string(),
    }
}

fn dir_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_string()
}

/// `<dir>/info.json` 里的 `(name, version)`。
fn info_json(dir: &Path) -> Option<(Option<String>, Option<String>)> {
    let raw = std::fs::read_to_string(dir.join("info.json")).ok()?;
    let value: serde_json::Value = serde_json::from_str(&raw).ok()?;
    Some((
        value.get("name")?.as_str().map(str::to_string),
        value
            .get("version")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
    ))
}

/// zip mod：返回 (mod 名, zip 内前缀, 版本)。
///
/// 条目形如 `aai-industry_0.7.3/graphics/…`，因此取第一个条目的第一段当前缀；
/// mod 名与版本优先用包里的 `info.json`，拿不到才退回前缀/文件名。
fn zip_mod_info(path: &Path) -> Option<(String, String, Option<String>)> {
    let file = File::open(path).ok()?;
    let mut zip = zip::ZipArchive::new(file).ok()?;
    let first = zip.by_index(0).ok()?.name().to_string();
    // mod zip 的条目都带 `<名字>_<版本>/` 前缀；`.modpack.zip` 这类包里直接躺着别的 zip
    //（第一个条目连目录分隔都没有），不能当 mod 收——否则会造出一个名字很长的幻影 mod。
    if !first.contains('/') {
        return None;
    }
    let prefix = first.split('/').next()?.to_string();
    if prefix.is_empty() {
        return None;
    }
    let mut info_name = None;
    let mut info_version = None;
    if let Ok(mut entry) = zip.by_name(&format!("{prefix}/info.json")) {
        let mut raw = String::new();
        if entry.read_to_string(&mut raw).is_ok()
            && let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw)
        {
            info_name = value
                .get("name")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            info_version = value
                .get("version")
                .and_then(|v| v.as_str())
                .map(str::to_string);
        }
    }
    let prefix_version = version_suffix(&prefix).map(str::to_string);
    // 版本：info.json → 退回 zip 文件名 `<名字>_<版本>.zip` 的 `<版本>` 段
    //（同样要求「长得像版本号」，否则 `some_mod.zip` 会给出假版本 `mod`）。
    let version = info_version.clone().or_else(|| {
        let stem = path.file_stem()?.to_str()?;
        version_suffix(stem).map(str::to_string)
    });
    // 既没有 `info.json`、前缀与文件名也都不像 `<名字>_<版本>`：这不是 mod zip。
    if info_name.is_none()
        && info_version.is_none()
        && version.is_none()
        && prefix_version.is_none()
    {
        return None;
    }
    let name = info_name.unwrap_or_else(|| {
        // 名字也按 `<名字>_<版本>` 拆（zip 前缀就是 `<名字>_<版本>`）。
        name_without_version(&prefix)
    });
    Some((name, format!("{prefix}/"), version.or(prefix_version)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// mod 版本：`info.json` 优先；包里没有 info.json 时退回 zip 文件名
    /// 一个临时 mod 目录 + 写 zip 的小工具。
    fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
        use std::io::Write;
        let file = File::create(path).expect("建 zip");
        let mut writer = zip::ZipWriter::new(file);
        let options: zip::write::SimpleFileOptions = Default::default();
        for (name, bytes) in entries {
            writer.start_file(*name, options).expect("写条目");
            writer.write_all(bytes).expect("写字节");
        }
        writer.finish().expect("收尾");
    }

    fn temp_mods_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("metatorio-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("建临时目录");
        dir
    }

    fn write_mod_dir(root: &Path, id: &str, info: &str) {
        let dir = root.join(id);
        std::fs::create_dir_all(&dir).expect("建 mod 目录");
        std::fs::write(dir.join("info.json"), info).expect("写 info.json");
    }

    fn find<'a>(mods: &'a [LoadedMod], name: &str) -> &'a LoadedMod {
        mods.iter()
            .find(|file| file.name == name)
            .unwrap_or_else(|| panic!("扫到 {name}（实得 {mods:?}）"))
    }

    /// zip 形态：文件名必须带版本号（`<名字>_<版本>.zip`）；版本优先 `info.json`，
    /// 没有 `info.json` 时用文件名里的版本段。
    #[test]
    fn zip_mods_need_a_version_in_the_file_name() {
        let dir = temp_mods_dir("zipmods");
        write_zip(
            &dir.join("with-info_1.2.3.zip"),
            &[
                (
                    "with-info_1.2.3/info.json",
                    br#"{"name":"with-info","version":"9.9.9"}"#,
                ),
                ("with-info_1.2.3/graphics/x.png", b"png"),
            ],
        );
        write_zip(
            &dir.join("bare-mod_4.5.6.zip"),
            &[("bare-mod_4.5.6/graphics/x.png", b"png")],
        );
        // 文件名没有版本号 → 不是 mod（`.modpack.zip`、`some_mod.zip` 都落在这一档）
        write_zip(&dir.join("some_mod.zip"), &[("some_mod/data.lua", b"--")]);
        // 条目名里连目录分隔都没有 → 不是 mod
        write_zip(
            &dir.join(".modpack.zip"),
            &[("alien-biomes-graphics_0.8.0.zip", b"zip")],
        );

        let mods = loaded_mods(&dir).expect("扫描");
        assert_eq!(
            mods.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(),
            vec!["bare-mod", "with-info"]
        );
        assert_eq!(find(&mods, "with-info").version.as_deref(), Some("9.9.9"));
        assert_eq!(find(&mods, "bare-mod").version.as_deref(), Some("4.5.6"));
        assert!(matches!(
            find(&mods, "with-info").archive,
            Archive::Zip { .. }
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 目录形态：目录名**就是** mod id（带版本号的目录**不收**），`info.json` 直接躺在目录下，
    /// 版本取 `info.json.version`；没有 `info.json` 的不算 mod。
    #[test]
    fn folder_mods_must_be_named_by_id_and_hold_info_json() {
        let dir = temp_mods_dir("dirmods");
        write_mod_dir(
            &dir,
            "bodyguard-companion-drones",
            r#"{"name":"bodyguard-companion-drones","version":"2.2.0"}"#,
        );
        // 带版本号的目录：游戏不接受，我们也不收
        write_mod_dir(
            &dir,
            "pyalienlife_3.1.0",
            r#"{"name":"pyalienlife","version":"3.1.0"}"#,
        );
        // 目录名虽然是 id，但没有 info.json → 不是 mod
        std::fs::create_dir_all(dir.join("no-info-folder")).unwrap();

        let mods = loaded_mods(&dir).expect("扫描");
        assert_eq!(
            mods.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(),
            vec!["bodyguard-companion-drones"]
        );
        assert_eq!(
            find(&mods, "bodyguard-companion-drones").version.as_deref(),
            Some("2.2.0")
        );
        assert!(matches!(
            find(&mods, "bodyguard-companion-drones").archive,
            Archive::Dir(_)
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 同名多个候选：没锁版本 → 取**最新版本**（按版本号比，不是字符串比）；
    /// 锁了版本 → 取那个版本；同版本时**目录形态优先**。
    #[test]
    fn duplicate_mods_pick_the_newest_or_the_pinned_version() {
        let dir = temp_mods_dir("moddup");
        // 同一 mod 的三个候选：zip 1.9.0、zip 1.10.0、目录（info.json 里写 1.9.0）
        write_zip(
            &dir.join("dup_1.9.0.zip"),
            &[(
                "dup_1.9.0/info.json",
                br#"{"name":"dup","version":"1.9.0"}"#,
            )],
        );
        write_zip(
            &dir.join("dup_1.10.0.zip"),
            &[(
                "dup_1.10.0/info.json",
                br#"{"name":"dup","version":"1.10.0"}"#,
            )],
        );
        write_mod_dir(&dir, "dup", r#"{"name":"dup","version":"1.9.0"}"#);

        // 没锁版本 → 1.10.0（字符串比会错选 1.9.0，所以这条同时在守版本号比较）
        let mods = loaded_mods(&dir).expect("扫描");
        assert_eq!(find(&mods, "dup").version.as_deref(), Some("1.10.0"));
        assert!(matches!(find(&mods, "dup").archive, Archive::Zip { .. }));

        // 锁到 1.9.0 → 取 1.9.0；同版本时目录形态优先
        std::fs::write(
            dir.join("mod-list.json"),
            r#"{"mods":[{"name":"dup","enabled":true,"version":"1.9.0"}]}"#,
        )
        .unwrap();
        let mods = loaded_mods(&dir).expect("扫描");
        assert_eq!(find(&mods, "dup").version.as_deref(), Some("1.9.0"));
        assert!(matches!(find(&mods, "dup").archive, Archive::Dir(_)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `<名字>_<版本>` 的版本段必须长得像版本号。
    #[test]
    fn version_suffix_requires_a_version_like_tail() {
        assert_eq!(version_suffix("pyalienlife_3.1.0"), Some("3.1.0"));
        assert_eq!(version_suffix("mod_2026.07.06"), Some("2026.07.06"));
        assert_eq!(version_suffix("some_mod"), None);
        // 实测这台机器上真有 `NoEmptyBarrels_3` 这个 mod：`3` 不是版本，名字也不能被截断
        assert_eq!(version_suffix("NoEmptyBarrels_3"), None);
        assert_eq!(name_without_version("NoEmptyBarrels_3"), "NoEmptyBarrels_3");
        assert_eq!(version_suffix("mod_1.2.x"), None);
        assert_eq!(name_without_version("mod_1.2.3"), "mod");
        assert_eq!(
            compare_versions(Some("1.10.0"), Some("1.9.0")),
            std::cmp::Ordering::Greater
        );
        assert_eq!(
            compare_versions(Some("1.0.0"), None),
            std::cmp::Ordering::Greater
        );
    }

    /// `mod-list.json`：只取 `enabled: true` 的名字，并读出锁定版本。
    #[test]
    fn read_mod_list_reads_enabled_and_pinned_version() {
        let dir = temp_mods_dir("modlist");
        std::fs::write(
            dir.join("mod-list.json"),
            r#"{"mods":[{"name":"base","enabled":true},{"name":"off","enabled":false},
                        {"name":"on","enabled":true,"version":"1.2.3"}]}"#,
        )
        .expect("写 mod-list");
        assert_eq!(
            enabled_mod_names(&dir),
            vec!["base".to_string(), "on".to_string()]
        );
        let entries = read_mod_list(&dir);
        assert_eq!(entries[2].version.as_deref(), Some("1.2.3"));
        assert!(entries[0].version.is_none());
        // 没有这个文件（或不是标准布局）→ 空，不猜。
        assert!(read_mod_list(&dir.join("nope")).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn split_spec_reads_mod_and_path() {
        assert_eq!(
            split_spec("__base__/graphics/icons/iron-plate.png"),
            Some(("base", "graphics/icons/iron-plate.png".to_string()))
        );
        assert_eq!(
            split_spec("__aai-industry__/graphics/x.png"),
            Some(("aai-industry", "graphics/x.png".to_string()))
        );
        // 非资源路径（没有 __ 前缀）不该被当成来源路径。
        assert_eq!(split_spec("graphics/x.png"), None);
        assert_eq!(split_spec("__base__"), None);
    }

    /// 双斜杠必须归一化：py 的数据里确实有 `graphics/icons//x.png`，
    /// 而 zip 条目是按名字精确匹配的，不归一化就会「缺文件」。
    #[test]
    fn split_spec_collapses_duplicate_slashes() {
        assert_eq!(
            split_spec("__pyalienlifegraphics__/graphics/icons//sap-extractor-mk01.png"),
            Some((
                "pyalienlifegraphics",
                "graphics/icons/sap-extractor-mk01.png".to_string()
            ))
        );
        assert_eq!(
            split_spec("__base__//graphics///icons/x.png"),
            Some(("base", "graphics/icons/x.png".to_string()))
        );
        assert_eq!(split_spec("__base__//"), Some(("base", String::new())));
    }

    /// 归一化后的路径要真的能读到文件（目录形式），空相对路径要报错而不是读整个目录。
    #[test]
    fn read_uses_the_normalized_path() {
        let dir = std::env::temp_dir().join(format!("metatorio-sources-{}", std::process::id()));
        let icons = dir.join("graphics/icons");
        std::fs::create_dir_all(&icons).expect("建临时目录");
        std::fs::write(icons.join("x.png"), b"png").expect("写临时文件");
        let sources = IconSources::from_roots([("t".to_string(), Archive::Dir(dir.clone()))]);
        assert_eq!(sources.read("__t__/graphics/icons//x.png").unwrap(), b"png");
        assert!(sources.read("__t__//").is_err());
        assert!(sources.read("__nope__/graphics/icons/x.png").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
