//! 真正的 headless 入口：只依赖 `metatorio-shell`，不链接 tauri/webview/GTK。
//!
//! 对外提供与 `metatorio-app --headless` 完全相同的 MCP 端点（同一个 shell、
//! 同一份 `AppState`、同一套工具），但没有窗口、没有 webview、没有事件循环。

use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use metatorio_shell::app::AppState;
use metatorio_shell::host::Host;
use metatorio_shell::mcp::{self, ServerConfig};

/// 与 Tauri 侧一致的 bundle identifier（决定 AppData 目录，保证与 GUI 共用上下文）。
const IDENTIFIER: &str = "com.mirac.metatorio-app";

/// 异星工厂规划工具：纯 MCP 服务（无界面）。
#[derive(Debug, Parser)]
#[command(name = "metatorio-headless", version, about, long_about = None)]
struct Cli {
    /// MCP 监听地址：默认 `127.0.0.1`（只有本机能连）。
    #[arg(long, env = "METATORIO_MCP_BIND", default_value_t = metatorio_shell::DEFAULT_MCP_BIND.to_string())]
    mcp_bind: String,

    /// MCP 端点端口。
    #[arg(long, env = "METATORIO_MCP_PORT", default_value_t = metatorio_shell::DEFAULT_MCP_PORT)]
    mcp_port: u16,

    /// 额外允许的 `Host`（可重复，或一个逗号分隔的环境变量）。
    #[arg(
        long = "mcp-allow-host",
        env = "METATORIO_MCP_ALLOW_HOSTS",
        value_delimiter = ','
    )]
    mcp_allow_hosts: Vec<String>,

    /// 对外暴露哪些 MCP 工具（逗号分隔；留空 = 全部）。
    #[arg(long = "mcp-tools", env = "METATORIO_MCP_TOOLS", value_delimiter = ',')]
    mcp_tools: Vec<String>,

    /// MCP 鉴权 token（非回环绑定时必须提供）。
    #[arg(long, env = "METATORIO_MCP_TOKEN")]
    mcp_token: Option<String>,
}

/// headless 的 `Host`：没有前端，事件是 no-op；应用数据目录与 Tauri 侧一致。
struct HeadlessHost {
    state: Arc<AppState>,
}

impl Host for HeadlessHost {
    fn state(&self) -> &AppState {
        &self.state
    }

    fn app_data_dir(&self) -> PathBuf {
        let base = if cfg!(target_os = "windows") {
            std::env::var_os("APPDATA").map(PathBuf::from)
        } else if cfg!(target_os = "macos") {
            std::env::var_os("HOME")
                .map(|home| PathBuf::from(home).join("Library/Application Support"))
        } else {
            std::env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .or_else(|| {
                    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share"))
                })
        };
        base.unwrap_or_else(|| PathBuf::from(".")).join(IDENTIFIER)
    }

    /// 没有前端：事件直接丢弃（不是错误）。
    fn emit(&self, _event: &str, _payload: serde_json::Value) {}
}

fn main() {
    let cli = Cli::parse();
    let state = Arc::new(AppState::default());
    let host: Arc<dyn Host> = Arc::new(HeadlessHost { state });

    let config = ServerConfig {
        bind: cli.mcp_bind.clone(),
        port: cli.mcp_port,
        token: cli.mcp_token.clone(),
        allow_hosts: cli.mcp_allow_hosts.clone(),
        tools: cli.mcp_tools.clone(),
    };
    println!(
        "切向量化 headless（无 GUI）：MCP 端点在 http://{}:{}{}",
        cli.mcp_bind,
        cli.mcp_port,
        metatorio_shell::MCP_PATH
    );

    // 起 MCP（独立线程 + 自带 tokio runtime），恢复缓存上下文。
    mcp::spawn_server(host.clone(), config);
    metatorio_shell::app::restore_contexts(&host);

    // 没有窗口/事件循环可跑：park 住主线程，等进程被结束。
    loop {
        std::thread::park();
    }
}
