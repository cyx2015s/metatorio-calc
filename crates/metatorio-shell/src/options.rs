//! 启动选项与 MCP 端点常量。
//!
//! 由 bin 的 CLI 解析（CLI > 环境变量 > 默认）后传入。这里不依赖 clap，
//! Options 因此是普通数据、可单测，也便于将来换解析器；参数的单一真相在这里，
//! MCP 服务端与求解调度器都只认它，不再各自去读环境变量。

/// MCP 端点的默认回环端口（--mcp-port / METATORIO_MCP_PORT 可覆盖）。
pub const DEFAULT_MCP_PORT: u16 = 8765;

/// 默认只监听回环：这个端点能建项目、改目标、跑规划，默认不该被局域网里任何设备
/// 碰到。要手机/其它设备接入就显式 --mcp-bind <本机 IP>（或 0.0.0.0），并且**必须**
/// 配 token（启动前校验，见 bin 的 validate）。
pub const DEFAULT_MCP_BIND: &str = "127.0.0.1";

/// MCP 服务挂在这个路径下（例如 http://127.0.0.1:8765/mcp）。
pub const MCP_PATH: &str = "/mcp";

/// 启动选项：由 bin 的 CLI 解析后传入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// MCP 监听地址；默认 127.0.0.1（只回环）。
    pub mcp_bind: String,
    /// MCP 端点端口。
    pub mcp_port: u16,
    /// 额外允许的 Host（用主机名/mDNS 名访问时填）。
    pub mcp_allow_hosts: Vec<String>,
    /// 对外暴露哪些 MCP 工具；空 = 全部。
    pub mcp_tools: Vec<String>,
    /// MCP Bearer token；None/空 = 不鉴权（仅回环兜底）。
    pub mcp_token: Option<String>,
    /// 单次求解的等待上限（毫秒）；None = 内置默认。
    pub solve_timeout_ms: Option<u64>,
    /// 无头：不创建窗口，只提供 MCP 端点。
    pub headless: bool,
    /// 是否启动 MCP 端点。默认开。
    pub mcp: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            mcp_bind: DEFAULT_MCP_BIND.to_string(),
            mcp_port: DEFAULT_MCP_PORT,
            mcp_allow_hosts: Vec::new(),
            mcp_tools: Vec::new(),
            mcp_token: None,
            solve_timeout_ms: None,
            headless: false,
            mcp: true,
        }
    }
}
