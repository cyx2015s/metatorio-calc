// Prevents additional console window on Windows in release, DO NOT REMOVE!!
// 无头模式（`--headless`）在 release 下同样没有控制台：它的输出请重定向到文件
// （`metatorio-app.exe --headless > log.txt`），或直接用 debug 构建在终端里跑。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! 切向量化（metatorio）入口：解析启动参数，然后交给 `metatorio_app_lib::run`。
//!
//! 参数优先级：**命令行 > 环境变量 > 默认值**（环境变量经 clap 的 `env` 读取，
//! 因此旧的环境变量用法继续有效）：
//!
//! ```text
//! metatorio-app                                   # GUI + 内置 MCP（127.0.0.1:8765）
//! metatorio-app --headless                        # 不建窗口，只提供 MCP
//! metatorio-app --mcp-token abc --mcp-bind 192.168.1.23   # 局域网/手机接入
//! metatorio-app --mcp-port 8799 --mcp-token abc   # 换端口 / 开鉴权
//! METATORIO_HEADLESS=1 metatorio-app              # 等价的无头写法
//! ```

use clap::Parser;

use metatorio_app_lib::{mcp, Options};

/// 异星工厂规划工具：GUI（默认）或纯 MCP 服务（`--headless`）。
#[derive(Debug, Parser)]
#[command(name = "metatorio-app", version, about, long_about = None)]
struct Cli {
    /// 不创建窗口，只提供 MCP 端点（无界面；Linux 仍会初始化 GTK，需要 xvfb）。
    #[arg(long, env = "METATORIO_HEADLESS")]
    headless: bool,

    /// MCP 监听地址：默认 `127.0.0.1`（只有本机能连）。要让手机/其它设备接入就填本机
    /// 局域网 IP（`192.168.1.23`）或 `0.0.0.0`（所有网卡）——**非回环必须配 token**。
    #[arg(long, env = "METATORIO_MCP_BIND", default_value_t = mcp::DEFAULT_MCP_BIND.to_string())]
    mcp_bind: String,

    /// MCP 端点端口。
    #[arg(long, env = "METATORIO_MCP_PORT", default_value_t = mcp::DEFAULT_MCP_PORT)]
    mcp_port: u16,

    /// 额外允许的 `Host`（可重复，或一个逗号分隔的环境变量）：用主机名/mDNS 名访问时
    /// 填，例如 `--mcp-allow-host mirac-pc.local`。IP 由 `--mcp-bind` 自动允许。
    #[arg(
        long = "mcp-allow-host",
        env = "METATORIO_MCP_ALLOW_HOSTS",
        value_delimiter = ','
    )]
    mcp_allow_hosts: Vec<String>,

    /// 对外暴露哪些 MCP 工具（逗号分隔；**留空 = 全部**）。工具越少，agent 选得越准：
    /// 例如 `--mcp-tools auto_plan,dispatch,get_planning_state` 只留「权威入口 + 万能
    /// 逃生通道 + 读取」。名字写错会拒绝启动并列出可用工具。
    #[arg(long = "mcp-tools", env = "METATORIO_MCP_TOOLS", value_delimiter = ',')]
    mcp_tools: Vec<String>,

    /// MCP 鉴权 token（`Authorization: Bearer <token>` 或裸 token）。
    /// 不提供 = 不鉴权，此时只允许监听回环地址。
    #[arg(long, env = "METATORIO_MCP_TOKEN")]
    mcp_token: Option<String>,

    /// 单次求解的等待上限（毫秒）；超时返回可重试错误，后台任务继续跑完。
    #[arg(long, env = "METATORIO_SOLVE_TIMEOUT_MS")]
    solve_timeout_ms: Option<u64>,

    /// 不启动 MCP 端点（只想要 GUI、不开本地端口时用）。
    #[arg(long = "no-mcp", env = "METATORIO_NO_MCP")]
    no_mcp: bool,
}

/// 参数组合校验。两条都是**拒绝启动**而不是警告：静默跑出一个「没有窗口也没有接口」
/// 的进程最难排查；而把能改文档、跑规划的端点暴露到回环之外却**没有 token**，等于把
/// 规划器交给整个局域网（家用网段里任何设备都能扫到）。
fn validate(cli: &Cli) -> Result<(), String> {
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

fn main() {
    let cli = Cli::parse();
    if let Err(message) = validate(&cli) {
        eprintln!("参数错误：{message}");
        std::process::exit(2);
    }
    metatorio_app_lib::run(Options {
        mcp_bind: cli.mcp_bind.clone(),
        mcp_port: cli.mcp_port,
        mcp_allow_hosts: cli.mcp_allow_hosts.clone(),
        mcp_tools: cli.mcp_tools.clone(),
        // 空串等于没给：与原来「env 存在但为空 → 不鉴权」的行为一致。
        mcp_token: cli.mcp_token.filter(|token| !token.trim().is_empty()),
        solve_timeout_ms: cli.solve_timeout_ms,
        headless: cli.headless,
        mcp: !cli.no_mcp,
    });
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;

    /// 端口必须能解析失败（否则会静默退化成默认端口）。
    #[test]
    fn invalid_port_is_rejected() {
        assert!(Cli::try_parse_from(["metatorio-app", "--mcp-port", "not-a-port"]).is_err());
        assert!(Cli::try_parse_from(["metatorio-app", "--solve-timeout-ms", "-1"]).is_err());
    }

    /// `--mcp-tools`：写错工具名必须**拒绝启动**（而不是静默全开或静默少开一个）。
    #[test]
    fn mcp_tools_selection_is_validated() {
        let cli = |tools: Vec<&str>| Cli {
            headless: true,
            mcp_bind: mcp::DEFAULT_MCP_BIND.to_string(),
            mcp_port: mcp::DEFAULT_MCP_PORT,
            mcp_allow_hosts: Vec::new(),
            mcp_tools: tools.into_iter().map(str::to_string).collect(),
            mcp_token: None,
            solve_timeout_ms: None,
            no_mcp: false,
        };

        // 留空 = 全部；群内 agent 推荐的三个核心；`all` 也是全部。
        assert!(validate(&cli(vec![])).is_ok());
        assert!(validate(&cli(vec!["all"])).is_ok());
        assert!(validate(&cli(vec!["auto_plan", "dispatch", "get_planning_state"])).is_ok());
        // 写错 → 报错，并列出可用工具。
        let err = validate(&cli(vec!["auto_plan", "nope"])).unwrap_err();
        assert!(err.contains("nope"), "{err}");
        assert!(err.contains("auto_plan"), "{err}");
    }

    /// `--headless --no-mcp` = 既没有窗口也没有接口：必须显式拒绝。
    ///
    /// 这里**直接构造 `Cli`**而不是 `try_parse_from`：clap 的 `env` 回退会把
    /// `METATORIO_HEADLESS` 读进来，而环境变量是进程级的——`cargo test` 同二进制内
    /// 并行跑时，另一个测试刚设上的 `METATORIO_HEADLESS=true` 会让这里的
    /// `--no-mcp` 子用例莫名变成「headless + no-mcp」。本断言只关心**组合规则**，
    /// 来源解析交给 `cli_overrides_env_over_defaults` 单独验证。
    #[test]
    fn headless_without_mcp_is_rejected() {
        let cli = |headless, no_mcp| Cli {
            headless,
            mcp_bind: mcp::DEFAULT_MCP_BIND.to_string(),
            mcp_port: mcp::DEFAULT_MCP_PORT,
            mcp_allow_hosts: Vec::new(),
            mcp_tools: Vec::new(),
            mcp_token: None,
            solve_timeout_ms: None,
            no_mcp,
        };

        assert!(validate(&cli(true, false)).is_ok());
        assert!(validate(&cli(false, true)).is_ok());
        assert!(validate(&cli(false, false)).is_ok());

        let err = validate(&cli(true, true)).unwrap_err();
        assert!(
            err.contains("headless"),
            "错误信息要说清是哪两条互斥：{err}"
        );
    }

    /// 暴露到回环之外必须带 token：不然整个家用网段里的设备都能建项目、跑规划。
    /// 这里也是唯一需要 `--mcp-bind` 解析的地方，所以顺带覆盖非法地址。
    #[test]
    fn non_loopback_bind_requires_a_token() {
        let cli = |bind: &str, token: Option<&str>, no_mcp| Cli {
            headless: false,
            mcp_bind: bind.to_string(),
            mcp_port: mcp::DEFAULT_MCP_PORT,
            mcp_allow_hosts: Vec::new(),
            mcp_tools: Vec::new(),
            mcp_token: token.map(str::to_string),
            solve_timeout_ms: None,
            no_mcp,
        };

        // 默认回环：不带 token 也放行（旧行为不变）。
        assert!(validate(&cli("127.0.0.1", None, false)).is_ok());
        assert!(validate(&cli("localhost", None, false)).is_ok());
        // 局域网 IP / 所有网卡：必须给 token。
        let err = validate(&cli("192.168.1.23", None, false)).unwrap_err();
        assert!(err.contains("token"), "{err}");
        assert!(validate(&cli("192.168.1.23", Some("secret"), false)).is_ok());
        assert!(validate(&cli("0.0.0.0", Some("secret"), false)).is_ok());
        let err = validate(&cli("0.0.0.0", None, false)).unwrap_err();
        assert!(err.contains("token"), "{err}");
        // 空串 token 等于没给（与 main 里 filter 后的语义一致）。
        assert!(validate(&cli("192.168.1.23", Some("   "), false)).is_err());
        // MCP 关掉时不管绑哪儿。
        assert!(validate(&cli("0.0.0.0", None, true)).is_ok());
        // 非法地址：连解析都过不去（而不是静默退化成回环）。
        let err = validate(&cli("not-an-ip", Some("secret"), false)).unwrap_err();
        assert!(err.contains("mcp-bind"), "{err}");
    }

    /// CLI > 环境变量 > 默认值，三条来源都要生效且优先级不能反。
    ///
    /// 环境变量是**进程级**的，因此这些断言必须放在同一个测试里串行做完，
    /// 否则并行跑的两个测试会互相污染（这正是它合并成一个测试的原因）。
    #[test]
    fn cli_overrides_env_over_defaults() {
        for key in [
            "METATORIO_MCP_PORT",
            "METATORIO_MCP_BIND",
            "METATORIO_MCP_ALLOW_HOSTS",
            "METATORIO_MCP_TOOLS",
            "METATORIO_MCP_TOKEN",
            "METATORIO_HEADLESS",
            "METATORIO_SOLVE_TIMEOUT_MS",
        ] {
            std::env::remove_var(key);
        }

        // 1) 默认值。
        let cli = Cli::try_parse_from(["metatorio-app"]).unwrap();
        assert_eq!(cli.mcp_bind, mcp::DEFAULT_MCP_BIND);
        assert_eq!(cli.mcp_port, mcp::DEFAULT_MCP_PORT);
        assert!(cli.mcp_allow_hosts.is_empty());
        assert!(!cli.headless);
        assert!(cli.mcp_token.is_none());
        assert!(cli.solve_timeout_ms.is_none());

        // 2) 全部来自命令行。
        let cli = Cli::try_parse_from([
            "metatorio-app",
            "--headless",
            "--mcp-port",
            "8799",
            "--mcp-bind",
            "192.168.1.23",
            "--mcp-allow-host",
            "mirac-pc.local",
            "--mcp-allow-host",
            "metatorio.local",
            "--mcp-token",
            "secret",
            "--solve-timeout-ms",
            "5000",
        ])
        .unwrap();
        assert!(cli.headless);
        assert_eq!(cli.mcp_port, 8799);
        assert_eq!(cli.mcp_bind, "192.168.1.23");
        assert_eq!(
            cli.mcp_allow_hosts,
            vec!["mirac-pc.local".to_string(), "metatorio.local".to_string()]
        );
        assert_eq!(cli.mcp_token.as_deref(), Some("secret"));
        assert_eq!(cli.solve_timeout_ms, Some(5000));

        // 3) 环境变量作为回退（旧用法继续有效）。
        std::env::set_var("METATORIO_MCP_PORT", "8801");
        std::env::set_var("METATORIO_MCP_BIND", "192.168.1.99");
        std::env::set_var("METATORIO_MCP_ALLOW_HOSTS", "a.local,b.local");
        std::env::set_var("METATORIO_MCP_TOOLS", "auto_plan,dispatch");
        std::env::set_var("METATORIO_MCP_TOKEN", "from-env");
        std::env::set_var("METATORIO_HEADLESS", "true");
        std::env::set_var("METATORIO_SOLVE_TIMEOUT_MS", "7000");
        let cli = Cli::try_parse_from(["metatorio-app"]).unwrap();
        assert_eq!(cli.mcp_port, 8801, "未给 CLI 时应回退到环境变量");
        assert_eq!(cli.mcp_bind, "192.168.1.99");
        assert_eq!(
            cli.mcp_allow_hosts,
            vec!["a.local".to_string(), "b.local".to_string()],
            "环境变量里的 Host 白名单按逗号分隔"
        );
        assert_eq!(
            cli.mcp_tools,
            vec!["auto_plan".to_string(), "dispatch".to_string()],
            "环境变量里的工具清单按逗号分隔"
        );
        assert_eq!(cli.mcp_token.as_deref(), Some("from-env"));
        assert!(cli.headless, "bool 标志也应支持环境变量");
        assert_eq!(cli.solve_timeout_ms, Some(7000));

        // 4) 命令行优先于环境变量。
        let cli = Cli::try_parse_from(["metatorio-app", "--mcp-port", "8802"]).unwrap();
        assert_eq!(cli.mcp_port, 8802);
        assert_eq!(
            cli.mcp_tools,
            vec!["auto_plan".to_string(), "dispatch".to_string()],
            "环境变量里的工具清单按逗号分隔"
        );
        assert_eq!(cli.mcp_token.as_deref(), Some("from-env"));

        // 5) 空 token = 不鉴权（main 里的 filter 把空串变成 None）。
        std::env::set_var("METATORIO_MCP_TOKEN", "");
        let cli = Cli::try_parse_from(["metatorio-app"]).unwrap();
        assert_eq!(cli.mcp_token.filter(|token| !token.trim().is_empty()), None);

        for key in [
            "METATORIO_MCP_PORT",
            "METATORIO_MCP_BIND",
            "METATORIO_MCP_ALLOW_HOSTS",
            "METATORIO_MCP_TOOLS",
            "METATORIO_MCP_TOKEN",
            "METATORIO_HEADLESS",
            "METATORIO_SOLVE_TIMEOUT_MS",
        ] {
            std::env::remove_var(key);
        }
    }
}
