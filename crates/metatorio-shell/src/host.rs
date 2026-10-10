//! GUI 外壳需要提供的能力。
//!
//! 核心逻辑（状态 + 命令/副作用循环 + MCP）刻意不依赖任何 GUI 框架；只有少数
//! 副作用必须由外壳实现（广播事件、应用数据目录、对话框、打开链接、更新器……）。
//! 把它们收敛到这个 trait，headless 与各个 GUI 前端就能复用同一套核心。

use std::path::PathBuf;

/// GUI 外壳能力。实现者通常是 Tauri app（持有 AppHandle）或 headless 顶层。
pub trait Host: Send + Sync + 'static {
    /// 应用数据目录：上下文注册表、图标缓存等都落在这里。
    fn app_data_dir(&self) -> PathBuf;

    /// 向前端广播一个事件。没有前端（headless）时应当是 no-op，而不是错误。
    fn emit(&self, event: &str, payload: serde_json::Value);
}
