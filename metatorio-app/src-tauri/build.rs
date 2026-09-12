/// Windows 测试二进制也要 comctl32 v6 的 manifest。
///
/// 为什么需要这一步：`tauri_build::build()` 通过 `embed-resource` 编译资源，而它发的是
/// `cargo:rustc-link-arg-bins=`——**只给 bin 目标**。lib 的测试二进制因此没有任何
/// manifest；一旦它链接进了 tao/wry 的窗口代码（tauri 的窗口栈对测试可达时就会），那些
/// 导入里就有 `TaskDialogIndirect`、`SetWindowSubclass`、`DefSubclassProc` 等
/// **comctl32 v6 才有的导出**，而没有 v6 manifest 时 Windows 会绑到 v5（System32 里那个），
/// 进程一启动就报 `STATUS_ENTRYPOINT_NOT_FOUND`（0xc0000139）——错误信息里连是哪个 DLL、
/// 哪个函数都不说，极难排查（本仓库 2026-xx 实测踩到：一个工具过滤的改动让窗口代码变得
/// 从测试可达，整个 `cargo test -p metatorio-app --lib` 全部加载失败）。
///
/// 这里给 **tests** 目标补一条 Common-Controls 依赖声明。bin 目标已有自己的资源清单，
/// `/MANIFESTDEPENDENCY` 只是往清单里追加依赖项，不会与它冲突。
#[cfg(windows)]
fn add_windows_test_manifest() {
    // 注意用**不带目标限定**的 rustc-link-arg：lib 的单元测试（`cargo test --lib`）不算
    // Cargo 的 “tests” 目标，`rustc-link-arg-tests` 对它无效（实测过）。
    println!(
        "cargo:rustc-link-arg=/MANIFESTDEPENDENCY:type='win32' \
         name='Microsoft.Windows.Common-Controls' version='6.0.0.0' \
         processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'"
    );
}

fn main() {
    tauri_build::build();
    #[cfg(windows)]
    add_windows_test_manifest();
}
