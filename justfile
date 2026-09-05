# 版本号同步工具。
# 用法:
#   just version major   # 主版本 +1（1.5.3 -> 2.0.0）
#   just version minor   # 次版本 +1（1.5.3 -> 1.6.0）
#   just version patch   # 补丁 +1（1.5.3 -> 1.5.4）
#   just version 1.6.0   # 直接指定目标版本号
# 同步范围: tauri.conf.json / Cargo.toml / package.json / Cargo.lock(metatorio-app)。
# 不触碰 workspace 其它 crate 的独立版本。

# Windows 下 just 默认找 `sh`；这里用 cmd 保证在 PowerShell/终端直接可跑。
# 在 Linux/macOS 上可改成: set shell := ["sh", "-cu"]
set shell := ["cmd.exe", "/C"]

# 默认动作：显示用法
default:
    @just --summary

# 同步应用版本号到所有版本来源文件
version target:
    node scripts/sync-version.mjs {{target}}
