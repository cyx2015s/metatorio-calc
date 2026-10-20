use crate::mcp;
use clap::Parser;
use std::path::{Path, PathBuf};

/// 异星工厂规划工具：GUI（默认）或纯 MCP 服务（`--headless`）。
#[derive(Debug, Parser)]
#[command(name = "metatorio-app", version, about, long_about = None)]
pub struct Cli {
    /// 要打开的工程文件；支持多个文件、空格与非 ASCII 路径。
    #[arg(value_name = "PROJECT", num_args = 0..)]
    pub files: Vec<PathBuf>,

    /// 不创建窗口，只提供 MCP 端点（无界面；Linux 仍会初始化 GTK，需要 xvfb）。
    #[arg(long, env = "METATORIO_HEADLESS")]
    pub headless: bool,

    /// MCP 监听地址：默认 `127.0.0.1`（只有本机能连）。要让手机/其它设备接入就填本机
    /// 局域网 IP（`192.168.1.23`）或 `0.0.0.0`（所有网卡）——**非回环必须配 token**。
    #[arg(long, env = "METATORIO_MCP_BIND", default_value_t = mcp::DEFAULT_MCP_BIND.to_string())]
    pub mcp_bind: String,

    /// MCP 端点端口。
    #[arg(long, env = "METATORIO_MCP_PORT", default_value_t = mcp::DEFAULT_MCP_PORT)]
    pub mcp_port: u16,

    /// 额外允许的 `Host`（可重复，或一个逗号分隔的环境变量）：用主机名/mDNS 名访问时
    /// 填，例如 `--mcp-allow-host mirac-pc.local`。IP 由 `--mcp-bind` 自动允许。
    #[arg(
        long = "mcp-allow-host",
        env = "METATORIO_MCP_ALLOW_HOSTS",
        value_delimiter = ','
    )]
    pub mcp_allow_hosts: Vec<String>,

    /// 对外暴露哪些 MCP 工具（逗号分隔；**留空 = 全部**）。工具越少，agent 选得越准：
    /// 例如 `--mcp-tools auto_plan,dispatch,get_planning_state` 只留「权威入口 + 万能
    /// 逃生通道 + 读取」。名字写错会拒绝启动并列出可用工具。
    #[arg(long = "mcp-tools", env = "METATORIO_MCP_TOOLS", value_delimiter = ',')]
    pub mcp_tools: Vec<String>,

    /// MCP 鉴权 token（`Authorization: Bearer <token>` 或裸 token）。
    /// 不提供 = 不鉴权，此时只允许监听回环地址。
    #[arg(long, env = "METATORIO_MCP_TOKEN")]
    pub mcp_token: Option<String>,

    /// 单次求解的等待上限（毫秒）；超时返回可重试错误，后台任务继续跑完。
    #[arg(long, env = "METATORIO_SOLVE_TIMEOUT_MS")]
    pub solve_timeout_ms: Option<u64>,

    /// 不启动 MCP 端点（只想要 GUI、不开本地端口时用）。
    #[arg(long = "no-mcp", env = "METATORIO_NO_MCP")]
    pub no_mcp: bool,
}

/// 参数组合校验。两条都是**拒绝启动**而不是警告：静默跑出一个「没有窗口也没有接口」
/// 的进程最难排查；而把能改文档、跑规划的端点暴露到回环之外却**没有 token**，等于把
/// 规划器交给整个局域网（家用网段里任何设备都能扫到）。
pub fn validate(cli: &Cli) -> Result<(), String> {
    if cli.headless && !cli.files.is_empty() {
        return Err(
            "工程文件启动参数用于 GUI；无头模式请通过 MCP 的 open-project 消息打开工程".to_string(),
        );
    }
    if cli.headless && cli.no_mcp {
        return Err("--headless 与 --no-mcp 不能同时使用：那样既没有窗口也没有 MCP".to_string());
    }
    if cli.no_mcp {
        // 端点没开，绑哪儿、有没有 token 都无所谓（不校验，免得拦下无害的组合）。
        return Ok(());
    }
    let (bind, _) = mcp::resolve_bind(&cli.mcp_bind, &cli.mcp_allow_hosts)?;
    // 工具名必须真实存在：写错就报错，而不是「静默全开」或「静默少开一个」。
    mcp::resolve_tools(&cli.mcp_tools)?;
    let has_token = cli
        .mcp_token
        .as_deref()
        .is_some_and(|token| !token.trim().is_empty());
    if !bind.is_loopback() && !has_token {
        return Err(format!(
            "--mcp-bind {} 会把 MCP 端点暴露到回环之外，必须同时提供 --mcp-token\
             （或 METATORIO_MCP_TOKEN）：这个端点能建项目、改目标、跑规划",
            cli.mcp_bind
        ));
    }
    Ok(())
}

/// Resolve forwarded arguments against the second process's working directory.
pub(crate) fn requested_paths(args: Vec<String>, cwd: &Path) -> Result<Vec<PathBuf>, String> {
    let cli = Cli::try_parse_from(args).map_err(|error| error.to_string())?;
    Ok(cli
        .files
        .into_iter()
        .map(|path| {
            if path.is_absolute() {
                path
            } else {
                cwd.join(path)
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn positional_files_support_spaces_unicode_multiple_and_double_dash() {
        let cli =
            Cli::try_parse_from(["app", "--no-mcp", "中文 空格.metatorio", "second.json"]).unwrap();
        assert_eq!(
            cli.files,
            [
                PathBuf::from("中文 空格.metatorio"),
                PathBuf::from("second.json")
            ]
        );
        let cli = Cli::try_parse_from(["app", "--", "-filename.metatorio"]).unwrap();
        assert_eq!(cli.files, [PathBuf::from("-filename.metatorio")]);
    }
    #[test]
    fn forwarded_paths_use_the_sending_process_directory_and_do_not_treat_options_as_files() {
        let cwd = std::env::temp_dir().join("发送端");
        let paths = requested_paths(
            vec![
                "app".into(),
                "--mcp-port".into(),
                "8799".into(),
                "a b.metatorio".into(),
            ],
            &cwd,
        )
        .unwrap();
        assert_eq!(paths, [cwd.join("a b.metatorio")]);
        assert!(
            requested_paths(vec!["app".into(), "--no-mcp".into()], &cwd)
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn headless_file_arguments_are_not_silently_ignored() {
        let cli = Cli::try_parse_from(["app", "--headless", "project.metatorio"]).unwrap();
        assert!(validate(&cli).unwrap_err().contains("GUI"));
    }
}
