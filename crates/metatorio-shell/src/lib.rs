//! metatorio-shell：跨平台外壳适配层、长任务作业调度与 MCP 服务集成。
//!
//! 本 crate 充当具体运行时宿主（Tauri 桌面应用与 Headless 守护进程）的统一业务承载层：
//! - **宿主接口抽象（[`Host`]）**：将只有宿主环境具备的系统副作用（事件广播、配置路径解析、
//!   原生文件对话框等）抽象为 trait，彻底解耦具体的桌面图形库（避免在 Linux 上强行链接 GTK/WebKit）；
//! - **多端代码复用**：相同的业务逻辑与状态树（[`AppState`](app::AppState)）同时服务于 Tauri 桌面 GUI
//!   和纯命令行无头服务（`metatorio-headless`）；
//! - **模型上下文协议（MCP）**：内置标准 MCP 服务端（[`mcp`]），支持外部 Agent（如 Claude Desktop 等）
//!   以工具调用的形式直接分析工厂网络与计算最优解；
//! - **异步求解作业池（[`solve_jobs`]）**：负责防抖、排队与执行大规模复杂拓扑的非阻塞后台求解，
//!   并通过事件流（[`events`]）向前端推送实时进度。

pub mod app;
mod auto_plan_progress;
pub mod host;
pub mod mcp;
pub mod network_png;
pub mod options;
pub mod prototype_calculation;
mod rich_text;
pub mod solve_jobs;
#[cfg(test)]
mod universe_tests;
pub mod update;

pub use host::Host;
pub use options::{DEFAULT_MCP_BIND, DEFAULT_MCP_PORT, MCP_PATH, Options};

pub mod events;
