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

    /// MCP 端点端口（只监听 127.0.0.1）。
    #[arg(long, env = "METATORIO_MCP_PORT", default_value_t = mcp::DEFAULT_MCP_PORT)]
    mcp_port: u16,

    /// MCP 鉴权 token（`Authorization: Bearer <token>` 或裸 token）。
    /// 不提供 = 不鉴权，仅靠 loopback 兜底。
    #[arg(long, env = "METATORIO_MCP_TOKEN")]
    mcp_token: Option<String>,

    /// 单次求解的等待上限（毫秒）；超时返回可重试错误，后台任务继续跑完。
    #[arg(long, env = "METATORIO_SOLVE_TIMEOUT_MS")]
    solve_timeout_ms: Option<u64>,

    /// 不启动 MCP 端点（只想要 GUI、不开本地端口时用）。
    #[arg(long = "no-mcp", env = "METATORIO_NO_MCP")]
    no_mcp: bool,
}

/// 参数组合校验：`--headless` 的全部意义就是那个 MCP 端点，所以它必须开着；
/// 静默跑出一个「没有窗口也没有接口」的进程是最难排查的形态。
fn validate(cli: &Cli) -> Result<(), String> {
    if cli.headless && cli.no_mcp {
        return Err("--headless 与 --no-mcp 不能同时使用：那样既没有窗口也没有 MCP".to_string());
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
        mcp_port: cli.mcp_port,
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
            mcp_port: mcp::DEFAULT_MCP_PORT,
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

    /// CLI > 环境变量 > 默认值，三条来源都要生效且优先级不能反。
    ///
    /// 环境变量是**进程级**的，因此这些断言必须放在同一个测试里串行做完，
    /// 否则并行跑的两个测试会互相污染（这正是它合并成一个测试的原因）。
    #[test]
    fn cli_overrides_env_over_defaults() {
        for key in [
            "METATORIO_MCP_PORT",
            "METATORIO_MCP_TOKEN",
            "METATORIO_HEADLESS",
            "METATORIO_SOLVE_TIMEOUT_MS",
        ] {
            std::env::remove_var(key);
        }

        // 1) 默认值。
        let cli = Cli::try_parse_from(["metatorio-app"]).unwrap();
        assert_eq!(cli.mcp_port, mcp::DEFAULT_MCP_PORT);
        assert!(!cli.headless);
        assert!(cli.mcp_token.is_none());
        assert!(cli.solve_timeout_ms.is_none());

        // 2) 全部来自命令行。
        let cli = Cli::try_parse_from([
            "metatorio-app",
            "--headless",
            "--mcp-port",
            "8799",
            "--mcp-token",
            "secret",
            "--solve-timeout-ms",
            "5000",
        ])
        .unwrap();
        assert!(cli.headless);
        assert_eq!(cli.mcp_port, 8799);
        assert_eq!(cli.mcp_token.as_deref(), Some("secret"));
        assert_eq!(cli.solve_timeout_ms, Some(5000));

        // 3) 环境变量作为回退（旧用法继续有效）。
        std::env::set_var("METATORIO_MCP_PORT", "8801");
        std::env::set_var("METATORIO_MCP_TOKEN", "from-env");
        std::env::set_var("METATORIO_HEADLESS", "true");
        std::env::set_var("METATORIO_SOLVE_TIMEOUT_MS", "7000");
        let cli = Cli::try_parse_from(["metatorio-app"]).unwrap();
        assert_eq!(cli.mcp_port, 8801, "未给 CLI 时应回退到环境变量");
        assert_eq!(cli.mcp_token.as_deref(), Some("from-env"));
        assert!(cli.headless, "bool 标志也应支持环境变量");
        assert_eq!(cli.solve_timeout_ms, Some(7000));

        // 4) 命令行优先于环境变量。
        let cli = Cli::try_parse_from(["metatorio-app", "--mcp-port", "8802"]).unwrap();
        assert_eq!(cli.mcp_port, 8802);
        assert_eq!(cli.mcp_token.as_deref(), Some("from-env"));

        // 5) 空 token = 不鉴权（main 里的 filter 把空串变成 None）。
        std::env::set_var("METATORIO_MCP_TOKEN", "");
        let cli = Cli::try_parse_from(["metatorio-app"]).unwrap();
        assert_eq!(cli.mcp_token.filter(|token| !token.trim().is_empty()), None);

        for key in [
            "METATORIO_MCP_PORT",
            "METATORIO_MCP_TOKEN",
            "METATORIO_HEADLESS",
            "METATORIO_SOLVE_TIMEOUT_MS",
        ] {
            std::env::remove_var(key);
        }
    }
}
