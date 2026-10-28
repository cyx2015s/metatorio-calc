# Rust / TypeScript IPC 合约

桌面端使用 [tauri-specta](https://github.com/specta-rs/tauri-specta) 从 Rust 命令、事件与 DTO 生成 TypeScript。它减少重复声明、命令参数拼写错误和协议漂移；底层仍是 Tauri invoke/event，不承诺降低 IPC 延迟。

## 修改流程

1. 在 Rust 修改 DTO 或命令。参与协议的类型派生 `specta::Type`；业务 crate 的派生由可选 `specta` feature 控制。
2. 新命令放入 `metatorio-app/src-tauri/src/bindings.rs` 的集中注册表；它同时供运行时 handler 和生成器使用。事件也在这里登记，保留原事件名称。
3. 在 `metatorio-app` 运行 `pnpm bindings`，提交更新后的 `src/lib/runtime/bindings.ts`。
4. 运行 `pnpm bindings:check`、`pnpm check` 及相关回归测试。CI 发布构建会检查绑定是否与 Rust 一致。

生成命令使用 `--no-default-features`，不必编译 SoPlex / HiGHS。Windows 仍需可用的 MSVC 开发环境。生成通过显式 ignored Rust 测试完成，不启动 GUI，也不在应用启动或普通前端构建时写源码。普通 Rust 测试只比较临时生成结果，不修改仓库。

## 文件职责

- `bindings.rs`：唯一的命令注册表、事件合约及导出/漂移检查。
- `bindings.ts`：自动生成，禁止手改；包含命令函数、事件订阅器以及输入/输出类型。
- `types.ts`：从生成类型选择输入/输出阶段，保留 UI 辅助函数与地点展示投影。机制通用编辑器的宽视图也从 Rust 联合类型推导，不另列协议字段。
- `client.ts`：浏览器环境检查、原有 UI 参数顺序适配、PNG Blob 编码及事件 payload 解包。
- `metatorio-shell::events`：进度及求解错误的 Rust 载荷，生产者使用同一 DTO 构造 JSON。

输入使用 `_Deserialize`，输出使用 `_Serialize`，因此 serde 默认字段和省略字段可以准确反映在前端。不要为消除类型报错而关闭 serde phases。浮点类型按 JSON 的实际行为导出为 `number | null`；展示代码处理不可用值，不能把它当作零。

## 兼容性选择

- 精确锁定 `tauri-specta = 2.0.0-rc.26`、`specta = 2.0.0-rc.26`、`specta-typescript = 0.0.13`。这些仍是预发布依赖，升级时应一起检查生成 diff 与回归结果。
- 显式使用 `ErrorHandlingMode::Throw`：保留原有 rejected Promise，错误仍是原 String / Diagnostic，不引入 `{status,data,error}` 包装。
- 显式使用 `dangerously_cast_bigints_to_number`：保持现有 ID / revision 的 JSON number 协议。它没有修复 JavaScript 超过 `Number.MAX_SAFE_INTEGER` 的精度限制；若将来迁移，应同时改变序列化、前端和持久化策略。
- `document-changed` 允许 revision 或 null（工程导入刷新）；自动规划事件保持 project/factory/universe/run_id 与完整进度字段。
- 核心、求解器及 headless 不依赖 Tauri。只有桌面外壳引入 tauri-specta，业务 crate 的 Type 元数据为可选功能。

新增通信回归测试覆盖参数映射、错误拒绝、事件原始载荷/取消订阅和浏览器环境检查。
