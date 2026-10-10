//! 真正的 headless 入口：只依赖 metatorio-shell，不链接 tauri/webview/GTK。
//!
//! 对外提供与 metatorio-app --headless 完全相同的 MCP 端点（同一个 shell、
//! 同一份 AppState、同一套工具），但没有窗口、没有 webview、没有事件循环。
//!
//! 更新：与 GUI 共用同一份 latest.json 和同一把 minisign 公钥，只是用不同的
//! target 键（headless-{os}-{arch}）。--check-update 只查，--self-update 查并就地
//! 替换自己（需要外部守护进程/手动重启才生效）。见 docs/updates.md。

use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use metatorio_shell::app::AppState;
use metatorio_shell::host::Host;
use metatorio_shell::mcp::{self, ServerConfig};

/// 与 Tauri 侧一致的 bundle identifier（决定 AppData 目录，保证与 GUI 共用上下文）。
const IDENTIFIER: &str = "com.mirac.metatorio-app";

/// 与 GUI 完全相同的更新源与公钥（见 metatorio-app/src-tauri/tauri.conf.json）。
const DEFAULT_UPDATE_ENDPOINT: &str =
    "https://github.com/cyx2015s/metatorio-calc/releases/latest/download/latest.json";
const DEFAULT_UPDATE_PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEE4OEFFQTI1RUM5QTc4RjIKUldUeWVKcnNKZXFLcUhvTzJPNUR1dlFKdCtGWERveFFJd3ZDMXpJYkwwNUozemsrdjVBVjBqQkUK";

/// 异星工厂规划工具：纯 MCP 服务（无界面）。
#[derive(Debug, Parser)]
#[command(name = "metatorio-headless", version, about, long_about = None)]
struct Cli {
    /// MCP 监听地址：默认 127.0.0.1（只有本机能连）。
    #[arg(long, env = "METATORIO_MCP_BIND", default_value_t = metatorio_shell::DEFAULT_MCP_BIND.to_string())]
    mcp_bind: String,

    /// MCP 端点端口。
    #[arg(long, env = "METATORIO_MCP_PORT", default_value_t = metatorio_shell::DEFAULT_MCP_PORT)]
    mcp_port: u16,

    /// 额外允许的 Host（可重复，或一个逗号分隔的环境变量）。
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

    /// 只检查更新并打印结果，然后退出。
    #[arg(long)]
    check_update: bool,

    /// 检查 + 下载 + 验签 + 就地替换自己，然后退出（需重启进程才生效）。
    #[arg(long)]
    self_update: bool,

    /// 更新清单地址（默认与 GUI 相同）。
    #[arg(long, env = "METATORIO_UPDATE_ENDPOINT", default_value = DEFAULT_UPDATE_ENDPOINT)]
    update_endpoint: String,

    /// minisign 公钥（默认与 GUI 相同）。
    #[arg(long, env = "METATORIO_UPDATE_PUBKEY", default_value = DEFAULT_UPDATE_PUBKEY)]
    update_pubkey: String,

    /// 覆盖清单里的 target 键（默认 headless-{os}-{arch}）。
    #[arg(long, env = "METATORIO_UPDATE_TARGET")]
    update_target: Option<String>,
}

/// headless 的 Host：没有前端，事件是 no-op；应用数据目录与 Tauri 侧一致。
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

/// 处理 --check-update / --self-update。返回 Some(exit_code) 表示应当退出。
fn handle_update(cli: &Cli) -> Option<i32> {
    if !cli.check_update && !cli.self_update {
        return None;
    }
    let current = env!("CARGO_PKG_VERSION");
    match metatorio_shell::update::check(
        &cli.update_endpoint,
        cli.update_target.as_deref(),
        current,
    ) {
        Err(error) => {
            eprintln!("检查更新失败：{error}");
            Some(1)
        }
        Ok(None) => {
            println!("已是最新版本 {current}");
            Some(0)
        }
        Ok(Some(release)) => {
            println!(
                "发现新版本 {}（当前 {current}）{}",
                release.version,
                release
                    .pub_date
                    .as_deref()
                    .map(|date| format!(" 发布于 {date}"))
                    .unwrap_or_default()
            );
            if let Some(notes) = &release.notes {
                println!("更新说明：{notes}");
            }
            if !cli.self_update {
                println!("（加 --self-update 可下载并替换本二进制）");
                return Some(0);
            }
            match metatorio_shell::update::install(&release, &cli.update_pubkey) {
                Ok(path) => {
                    println!("已替换：{}（重启进程后生效）", path.display());
                    Some(0)
                }
                Err(error) => {
                    eprintln!("安装更新失败：{error}");
                    Some(1)
                }
            }
        }
    }
}

fn main() {
    let cli = Cli::parse();
    if let Some(code) = handle_update(&cli) {
        std::process::exit(code);
    }

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

    mcp::spawn_server(host.clone(), config);
    metatorio_shell::app::restore_contexts(&host);

    loop {
        std::thread::park();
    }
}
