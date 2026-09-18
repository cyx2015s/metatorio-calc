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

    /// `<游戏>/data/{base,core}` + mod 目录（默认 `<游戏>/mods`）。
    ///
    /// `data/` 下的**每个子目录**都注册成一个来源：DLC（Space Age）就是
    /// `<游戏>/data/space-age`，图标里写的是 `__space-age__/…`，与 base/core 同级。
    pub fn from_game_root(game: &Path, mod_dir: Option<&Path>) -> Result<Self, String> {
        let data = game.join("data");
        let base = data.join("base");
        if !base.is_dir() {
            return Err(format!("找不到游戏数据目录: {}", base.display()));
        }
        let mods = mod_dir
            .map(Path::to_path_buf)
            .unwrap_or_else(|| game.join("mods"));
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
        for (name, archive) in scan_mods(&mods)? {
            sources.roots.insert(name, archive);
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

/// 扫描 mod 目录：解压出来的目录、以及 `<名字>_<版本>.zip`。
///
/// mod 名优先取 `info.json` 里的 `name`（zip 里读、目录里读），拿不到才退回文件名。
fn scan_mods(dir: &Path) -> Result<Vec<(String, Archive)>, String> {
    let mut found = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        // 没有 mod 目录是正常的（纯原版）。
        Err(_) => return Ok(found),
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = dir_name(&path);
            let name = info_json_name(&path).unwrap_or(name);
            found.push((name, Archive::Dir(path)));
        } else if path.extension().is_some_and(|ext| ext == "zip") {
            let file_name = dir_name(&path);
            match zip_mod_name(&path) {
                Some((name, prefix)) => found.push((name, Archive::Zip { path, prefix })),
                // 不是 mod zip（例如 `.modpack.zip`）：跳过。
                None => {
                    let _ = file_name;
                }
            }
        }
    }
    Ok(found)
}

fn dir_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_string()
}

/// `<dir>/info.json` 里的 `name` 字段。
fn info_json_name(dir: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(dir.join("info.json")).ok()?;
    let value: serde_json::Value = serde_json::from_str(&raw).ok()?;
    value.get("name")?.as_str().map(str::to_string)
}

/// zip mod：返回 (mod 名, zip 内前缀)。
///
/// 条目形如 `aai-industry_0.7.3/graphics/…`，因此取第一个条目的第一段当前缀；
/// mod 名优先用包里的 `info.json`。
fn zip_mod_name(path: &Path) -> Option<(String, String)> {
    let file = File::open(path).ok()?;
    let mut zip = zip::ZipArchive::new(file).ok()?;
    let first = zip.by_index(0).ok()?.name().to_string();
    let prefix = first.split('/').next()?.to_string();
    if prefix.is_empty() {
        return None;
    }
    let info_name = format!("{prefix}/info.json");
    let name = zip
        .by_name(&info_name)
        .ok()
        .and_then(|mut entry| {
            let mut raw = String::new();
            entry.read_to_string(&mut raw).ok()?;
            let value: serde_json::Value = serde_json::from_str(&raw).ok()?;
            value.get("name")?.as_str().map(str::to_string)
        })
        .unwrap_or_else(|| prefix.split('_').next().unwrap_or(&prefix).to_string());
    Some((name, format!("{prefix}/")))
}

#[cfg(test)]
mod tests {
    use super::*;

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
