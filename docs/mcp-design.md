# MCP 服务端设计

Metatorio 通过 `rmcp`（Streamable-HTTP，路径 `/mcp`）提供 MCP 服务。`metatorio-app`（桌面端）与 `metatorio-headless`（无头服务端）共用 `metatorio-shell::mcp` 的同一套实现。

- 在桌面端启用时，MCP 与 GUI 共享同一份 `Arc<AppState>`（同一把 `Mutex<Runtime>`），外部 Agent 写入后会触发 `document-changed` 广播，界面实时刷新。
- 仅使用无头命令行服务（不需要窗口或桌面环境）时，运行独立二进制 `metatorio-headless`；它不链接 Tauri/WebView/GTK。

## 启动参数与安全边界

| 参数 | 环境变量 | 默认值 | 说明 |
| :--- | :--- | :--- | :--- |
| `--mcp-bind` | `METATORIO_MCP_BIND` | `127.0.0.1` | 监听地址；绑定非回环地址时必须同时提供 token，否则拒绝启动 |
| `--mcp-port` | `METATORIO_MCP_PORT` | `8765` | 监听端口 |
| `--mcp-token` | `METATORIO_MCP_TOKEN` | 无 | `Authorization: Bearer <token>` 或裸 token；回环地址允许省略 |
| `--mcp-allow-host` | `METATORIO_MCP_ALLOW_HOSTS` | 回环 / 绑定 IP | 逗号分隔的 `Host` 白名单（DNS rebinding 防护）；绑定 `0.0.0.0` / `::` 时关闭白名单 |
| `--mcp-tools` | `METATORIO_MCP_TOOLS` | 全部 | 逗号分隔的启用工具列表（名称错误直接拒绝启动） |
| `--solve-timeout-ms` | `METATORIO_SOLVE_TIMEOUT_MS` | `120000` | 同步命令等待求解的超时上限（毫秒） |

桌面端额外支持 `--headless`（隐藏窗口运行 MCP，与 `--no-mcp` 互斥）与 `--no-mcp`。

## 工具面

工具描述与校验报错均使用中文，所有流量、速率与目标数值统一为 **每秒**（项目 `time-scale` 只影响界面显示）。

| 工具 | 定位 | 核心行为 |
| :--- | :--- | :--- |
| `auto_plan` | 一站式自动规划 | 一次完成建项目/工厂、设置星球/主品质/目标/外部输入/插件/插件塔，先校验原型名，随后异步规划并立即返回 `project`/`factory` 与 `running` 状态，由 `get_planning_state` 轮询结果 |
| `dispatch` | 底层消息逃生通道 | 接收完整 `AppMessage`（基于 `schemars` 自动派生 `JsonSchema`），支持 `request_id` 幂等重放，返回 `revision`/`changed`/`created`/`scheduled_commands`/分页后的 `solve` 与结构化 `diagnostics` |
| `get_planning_state` | 逐层只读查询 | 无 `project` 查项目索引；仅 `project` 查设置与工厂/宇宙索引；`project + factory`（或 `universe`）查文档与 `auto_plan` 状态；追加 `mechanic` 时聚焦单个机制的 `config` 与单位每秒 `inputs`/`outputs`；支持 `recompute` |
| `list_contexts` | 上下文列表 | 列出已缓存的游戏数据上下文（ID、版本、启用 Mod、激活状态） |
| `list_prototypes` | 原型词表查询 | 按 `kind` 与 `name_contains` 分页检索上下文中的物品、流体、配方、机器、科技、星球等 |
| `localized_names` | 名称与 ID 互查 | 支持混用原型 ID 与本地化名称，忽略 `-` / `_` / 空白，依次按精确、前缀、子串和编辑距离给出匹配与错拼候选 |

### 设计约束

- **输出有界与显式分页**：所有列表与求解集合均受 `limit`（默认 50，上限 1000）和 `offset` 限制，响应附带 `page: { offset, limit, totals, truncated }`，不静默截断。
- **按需裁剪工具面**：可通过 `--mcp-tools auto_plan,dispatch,get_planning_state` 只暴露核心读写与规划入口；被禁用的工具不会出现在 `tools/list` 中。
- **异步状态与锁纪律**：长时求解与自动规划在锁外运行并受版本戳校验，详见 [Runtime 并发与锁纪律](runtime-concurrency.md)。

## 局域网与移动端接入

若需从手机或局域网内其他设备（如支持 Streamable HTTP 的客户端）接入：

1. 启动时绑定本机局域网 IP 并设置随机 token：
   ```text
   metatorio-app --mcp-bind 192.168.0.101 --mcp-token <token> --mcp-tools auto_plan,dispatch,get_planning_state
   ```
   放行对应端口的防火墙入站规则；启动日志会打印可访问的 URL 与 Host 白名单。
2. 客户端传输类型选择 **Streamable HTTP**，URL 填 `http://<IP>:<port>/mcp`，请求头添加 `Authorization: Bearer <token>`。
3. 排查顺序：访问端点返回 405/400 说明网络已通 → 401 检查 `Authorization` 头 → 403 检查 `Host` 白名单（可用 `--mcp-allow-host` 放行主机名）→ 超时检查防火墙与网段隔离。
