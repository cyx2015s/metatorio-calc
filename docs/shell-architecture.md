# 分层与「核心 / 外壳」不变量

这份文档记录一次重构后的边界：**业务核心与 GUI 外壳彻底分开**。它约束的是
「谁能依赖谁」，而不是某个具体功能怎么做。

## 分层

```
┌──────────────────────────┐   ┌──────────────────────────┐
│ metatorio-app (Tauri)    │   │ metatorio-headless       │
│  窗口 / 插件 / 对话框     │   │  纯 MCP 服务（无前端）    │
│  TauriHost               │   │  HeadlessHost            │
└───────────┬──────────────┘   └───────────┬──────────────┘
            │        Arc<dyn Host>          │
            └──────────────┬────────────────┘
                           ▼
              ┌────────────────────────────┐
              │ metatorio-shell            │
              │  AppState / 上下文 / 目录    │
              │  命令实现 / 副作用循环 / MCP  │
              └───────────┬────────────────┘
                          ▼
     metatorio-core / data / solver / icons / runtime
```

## 不变量（被破坏时要当成 bug）

1. **核心永不依赖 GUI crate。** `metatorio-shell` 及它下面的所有 crate，
   依赖树里不能出现 `tauri` / `wry` / `webkit2gtk` / `gtk` / `egui` …
   验证：`cargo tree -p metatorio-headless -e normal | Select-String 'tauri|gtk|webkit|wry'`
   必须为空。这条保证了 Linux 上的 headless 不需要 GTK/xvfb。
2. **GUI 外壳可替换。** 任何 GUI 只需实现 [`Host`] 并提供窗口，就能复用整套
   逻辑；`metatorio-app` 只是其中一种（Tauri）。
3. **唯一状态源。** 同一进程里只有一份 `Arc<AppState>`：GUI 命令、Tauri
   setup、MCP 工具处理器拿到的都是它。不要各自 `new` 一个。
4. **命令只有一份实现。** 逻辑写在 `metatorio-shell::app`（`pub` 函数，
   签名 `(&Arc<dyn Host>, &AppState, …)`）；GUI 侧只做参数转换的薄包装。
   不要在外壳里复制业务逻辑。

## `Host`：外壳能力的最小集合

核心只通过这些方法触碰「只有外壳能做的事」：

```rust
pub trait Host: Send + Sync + 'static {
    fn state(&self) -> &AppState;                 // 共享状态
    fn app_data_dir(&self) -> PathBuf;            // 落盘位置
    fn emit(&self, event: &str, payload: Value);  // 通知前端（无前端时 no-op）
}
```

- 对话框 / 打开链接 / 更新器 / 窗口**不在** trait 里：它们天然属于某个具体
  GUI，留在各自外壳（`metatorio-app` 的 `pick_*` 命令）。
- 若将来 headless 也要某个外壳能力（例如主动打开文件），再往 trait 上加，
  并在 `HeadlessHost` 里给出合理降级（返回 `None` / no-op），不要塞 GUI 类型。

## 为什么是 `Arc<dyn Host>` 而不是引用

shell 与 tauri 不保证同一线程，也不保证谁先销毁；两边都可能把任务丢进线程池
（`spawn_blocking`）里继续用状态。因此统一用 `Arc` 共享，绝不用借用跨过
异步/线程边界。

## 多前端 / 多包发布

`metatorio-app` 与 `metatorio-headless` 是**两个独立包/二进制**：

- GUI 走 `tauri-plugin-updater`；
- headless 可以不带更新器，或者也走 Tauri updater——**用自定义 `target`**
  区分（例如 `windows-x86_64-gui` / `windows-x86_64-headless`），同一个
  endpoint 也能各取各的清单，互不干扰。

## 文件地图

| 关注点 | 位置 |
|---|---|
| 共享状态 `AppState`、上下文注册表、目录/详情/本地化 | `crates/metatorio-shell/src/app.rs` |
| 19 个命令实现（`pub` 函数） | 同上，文件末尾 |
| 副作用循环 `execute_command`、自动规划 | 同上 |
| MCP 服务端（rmcp + axum） | `crates/metatorio-shell/src/mcp.rs` |
| `Host` trait | `crates/metatorio-shell/src/host.rs` |
| 启动选项 / 端点常量 | `crates/metatorio-shell/src/options.rs` |
| 求解调度器 | `crates/metatorio-shell/src/solve_jobs.rs` |
| Tauri 适配（`TauriHost` + 薄包装 + `run`） | `metatorio-app/src-tauri/src/lib.rs` |
| headless 入口（`HeadlessHost`） | `crates/metatorio-headless/src/main.rs` |
