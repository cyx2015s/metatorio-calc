//! GUI 无关的自更新支持（纯 Rust，不依赖 tauri）。
//!
//! 与 tauri-plugin-updater **共用同一份静态清单、同一把 minisign 公钥**，只用
//! 自定义 target 区分包：GUI 用默认的 {os}-{arch}，headless 用
//! headless-{os}-{arch}。清单格式（latest.json）里每个平台一条：
//!
//! ```text
//! { "signature": "<minisign .sig 的 base64>", "url": "<产物下载地址>" }
//! ```
//!
//! 键名就是 target：windows-x86_64 / headless-windows-x86_64 ……
//! 见 docs/updates.md。

use std::collections::HashMap;
use std::path::PathBuf;

use base64::Engine;
use serde::Deserialize;

/// 清单里的一条平台产物（字段与 tauri-plugin-updater 一致）。
#[derive(Debug, Clone, Deserialize)]
pub struct PlatformEntry {
    pub signature: String,
    pub url: String,
}

/// 静态更新清单。
#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    pub version: String,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub pub_date: Option<String>,
    #[serde(default)]
    pub platforms: HashMap<String, PlatformEntry>,
}

/// 解析出的「可用更新」。
#[derive(Debug, Clone)]
pub struct Release {
    pub version: semver::Version,
    pub notes: Option<String>,
    pub pub_date: Option<String>,
    pub url: String,
    pub signature: String,
}

/// headless 二进制的清单 target 键：headless-{os}-{arch}。
/// 与 tauri 一样把 macOS 写成 darwin。
pub fn default_target() -> String {
    let os = match std::env::consts::OS {
        "macos" => "darwin",
        other => other,
    };
    format!("headless-{os}-{}", std::env::consts::ARCH)
}

/// 从清单文本里挑出适用于 target 的更新（仅当比 current 新）。
pub fn parse_manifest(body: &str, target: &str, current: &str) -> Result<Option<Release>, String> {
    let manifest: Manifest =
        serde_json::from_str(body).map_err(|error| format!("清单解析失败: {error}"))?;
    let Some(entry) = manifest.platforms.get(target) else {
        let mut keys: Vec<_> = manifest.platforms.keys().cloned().collect();
        keys.sort();
        return Err(format!("清单里没有 target {target}（现有：{keys:?}）"));
    };
    let version = semver::Version::parse(manifest.version.trim_start_matches('v'))
        .map_err(|error| format!("版本号无效: {error}"))?;
    let current = semver::Version::parse(current.trim_start_matches('v'))
        .map_err(|error| format!("当前版本号无效: {error}"))?;
    if version > current {
        Ok(Some(Release {
            version,
            notes: manifest.notes,
            pub_date: manifest.pub_date,
            url: entry.url.clone(),
            signature: entry.signature.clone(),
        }))
    } else {
        Ok(None)
    }
}

/// 按 tauri-plugin-updater 的做法验签：公钥与签名都是 base64 包了一层的
/// minisign 文本（untrusted comment 那一行）。
pub fn verify(data: &[u8], signature_b64: &str, pubkey_b64: &str) -> Result<(), String> {
    let engine = base64::engine::general_purpose::STANDARD;
    let decode = |text: &str| -> Result<String, String> {
        let bytes = engine.decode(text).map_err(|error| error.to_string())?;
        String::from_utf8(bytes).map_err(|error| error.to_string())
    };
    let public_key =
        minisign_verify::PublicKey::decode(&decode(pubkey_b64)?).map_err(|e| e.to_string())?;
    let signature =
        minisign_verify::Signature::decode(&decode(signature_b64)?).map_err(|e| e.to_string())?;
    public_key
        .verify(data, &signature, true)
        .map_err(|error| format!("签名校验失败: {error}"))
}

fn client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .user_agent(concat!("metatorio-headless/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|error| error.to_string())
}

/// 取回清单文本。
pub fn fetch(endpoint: &str) -> Result<String, String> {
    client()?
        .get(endpoint)
        .send()
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?
        .text()
        .map_err(|error| error.to_string())
}

/// 下载产物字节。
pub fn download(url: &str) -> Result<Vec<u8>, String> {
    let bytes = client()?
        .get(url)
        .send()
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?
        .bytes()
        .map_err(|error| error.to_string())?;
    Ok(bytes.to_vec())
}

/// 只检查、不安装。target 为 None 时用 default_target。
pub fn check(
    endpoint: &str,
    target: Option<&str>,
    current: &str,
) -> Result<Option<Release>, String> {
    let target = target.map(str::to_string).unwrap_or_else(default_target);
    parse_manifest(&fetch(endpoint)?, &target, current)
}

/// 下载 + 验签 + 就地替换当前可执行文件，返回其路径（**调用方需重启进程才生效**）。
///
/// Windows 允许给运行中的 exe 改名，所以先 exe → exe.old 再写入新的；旧文件
/// 留作回滚，进程退出后可删。
pub fn install(release: &Release, pubkey: &str) -> Result<PathBuf, String> {
    let bytes = download(&release.url)?;
    verify(&bytes, &release.signature, pubkey)?;
    let exe = std::env::current_exe().map_err(|error| error.to_string())?;
    let staged = exe.with_extension("new");
    std::fs::write(&staged, &bytes).map_err(|error| error.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755))
            .map_err(|error| error.to_string())?;
    }
    let backup = exe.with_extension("old");
    let _ = std::fs::remove_file(&backup);
    std::fs::rename(&exe, &backup).map_err(|error| format!("备份旧文件失败: {error}"))?;
    std::fs::rename(&staged, &exe).map_err(|error| format!("替换可执行文件失败: {error}"))?;
    Ok(exe)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = r#"{
        "version": "1.6.0",
        "notes": "test",
        "platforms": {
            "windows-x86_64":          { "signature": "SIG-GUI", "url": "https://x/gui.exe" },
            "headless-windows-x86_64": { "signature": "SIG-HEAD", "url": "https://x/head.exe" }
        }
    }"#;

    #[test]
    fn picks_the_headless_target_and_ignores_gui_entries() {
        let release = parse_manifest(MANIFEST, "headless-windows-x86_64", "1.5.24")
            .unwrap()
            .expect("应有更新");
        assert_eq!(release.version.to_string(), "1.6.0");
        assert_eq!(release.signature, "SIG-HEAD");
        assert_eq!(release.url, "https://x/head.exe");
    }

    #[test]
    fn same_or_older_version_is_not_an_update() {
        assert!(
            parse_manifest(MANIFEST, "headless-windows-x86_64", "1.6.0")
                .unwrap()
                .is_none()
        );
        assert!(
            parse_manifest(MANIFEST, "headless-windows-x86_64", "2.0.0")
                .unwrap()
                .is_none()
        );
        assert!(
            parse_manifest(MANIFEST, "headless-windows-x86_64", "v1.6.0")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn missing_target_reports_available_keys() {
        let error = parse_manifest(MANIFEST, "headless-linux-x86_64", "1.5.0").unwrap_err();
        assert!(error.contains("headless-linux-x86_64"), "{error}");
        assert!(error.contains("windows-x86_64"), "{error}");
    }

    #[test]
    fn signature_verification_rejects_garbage() {
        let err = verify(b"hello", "AAAA", "AAAA").unwrap_err();
        assert!(!err.is_empty());
    }

    #[test]
    fn default_target_is_headless_prefixed() {
        let target = default_target();
        assert!(target.starts_with("headless-"), "{target}");
        assert!(target.contains(std::env::consts::ARCH), "{target}");
    }
}
