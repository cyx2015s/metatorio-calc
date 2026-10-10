//! GUI 无关的 shell 层：状态、命令/副作用循环、MCP 服务端。
//!
//! 这个 crate **刻意不依赖任何 GUI 框架**。GUI 外壳通过 [`Host`] 提供"只有外壳
//! 能做"的副作用（广播事件、对话框、打开链接、更新器……）。这样同一个 shell
//! 既能被 Tauri 前端复用，也能被真正的 headless 二进制复用——后者因此不必链接
//! tauri/webview（Linux 上也就不会被迫链接 GTK）。

pub mod app;
pub mod host;
pub mod options;
pub mod solve_jobs;

pub use host::Host;
pub use options::{DEFAULT_MCP_BIND, DEFAULT_MCP_PORT, MCP_PATH, Options};
