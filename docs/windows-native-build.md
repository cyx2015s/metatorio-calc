# Windows 原生依赖构建排查指南

在 Windows 环境下构建工作区时，由于底层引入了 C++ 求解器依赖（SoPlex 7.1.2 与 HiGHS，两者均通过 Rust `cmake` crate 驱动构建系统），可能遇到特定的生成器与沙箱权限问题。本文档记录了关键排查点与处置策略。

---

## 1. CMake 生成器版本不兼容

### 问题现象
当系统 PATH 中安装的 CMake 版本（如 4.x）其内置 Visual Studio 生成器列表最高支持到 `Visual Studio 17 2022`，但构建脚本自动探测到的 MSVC 工具链引发 CMake 尝试选用尚不存在的生成器名称（如 `Visual Studio 18 2026`）时，构建将直接中断报错：

```text
Could not create named generator Visual Studio 18 2026
```

### 处置方案
在已安装 Ninja 的 PowerShell 会话中，显式指定 CMake 生成器为 `Ninja`：

```powershell
$env:CMAKE_GENERATOR = 'Ninja'
cargo test --workspace --all-targets --no-fail-fast
```

> [!NOTE]
> 该现象纯属 CMake 宿主生成器的探测机制问题，与 SoPlex 的数值算法实现、Boost 头文件或链接库名无关。
> 项目未在仓库或全局环境中硬编码生成器，以便保留跨平台工具链的灵活性。

若旧的构建目录已缓存了错误的 CMake 配置，需要先清理相关原生包的构建缓存再重试：
```powershell
cargo clean -p soplex-sys -p highs-sys
```

---

## 2. 隔离沙箱中的 MSVC PDB 服务通信

### 问题现象
在使用 Ninja 生成器后，CMake 在进行 C/C++ 编译器特征探测时会默认传入 `/Zi /FS` 参数。在部分受限的进程隔离沙箱（Sandbox）内，PDB 服务进程之间的 RPC 通信可能被拦截，导致编译器报错 `C1902: 内部编译器错误 / 程序数据库不匹配` 或产生无限期挂起。

### 实测诊断
在外部标准终端（VS 开发者命令提示符或普通 PowerShell）中，同样的 MSVC 编译器、Ninja 与 `/Zi /FS` 探测命令均能秒级成功编译；而在受限沙箱中探测同等最小 C++ 程序时会异常挂起。

### 处置策略
- 确保执行原生依赖构建的终端具备跨进程通信（IPC/RPC）权限；
- 避免盲目修改项目的全局调试信息参数（如强行关闭 `/Zi`），保持生产级调试符号的完整性。

---

## 3. 常见构建差异速查

| 因素 | 受限沙箱环境 | 普通 PowerShell / VS 命令行 | 说明 |
| :--- | :--- | :--- | :--- |
| **PDB 通信** | 可能受 IPC 拦截阻止 | 完全放行，正常编译 | MSVC `/Zi /FS` 依赖并发 PDB 服务 |
| **CMake 生成器** | 易受环境变量波动影响 | 遵循全局配置或当前会话环境变量 | 推荐统一使用 `Ninja` 加速 |
| **构建缓存** | 建议在配置变更后针对性清理 | 增量编译有效 | 避免随意 `cargo clean` 引起全量重编 |
