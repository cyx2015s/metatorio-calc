# Windows 原生依赖构建排查

2026-10-28 在本机重新构建工作区时，发现两层独立问题。SoPlex 和 HiGHS 都通过 Rust `cmake` crate 构建，以下生成器问题同时影响两者。

## CMake 生成器版本

当前 PATH 中的 CMake 为 4.1.2，其生成器列表最高支持 Visual Studio 17 2022；自动探测到的 Visual Studio 安装却使构建脚本选择 `Visual Studio 18 2026`。首次配置因此报：

```text
Could not create named generator Visual Studio 18 2026
```

可在装有 Ninja 的当前 PowerShell 会话中设置：

```powershell
$env:CMAKE_GENERATOR = 'Ninja'
cargo test --workspace --all-targets --no-fail-fast
```

这不是 SoPlex 的数值实现、Boost 头文件或链接库名问题，无需修改求解算法。没有给仓库或用户全局环境强制设置生成器，以保留其它平台和工具链的选择。

若旧构建目录已经缓存了另一种生成器，需先清理对应包的构建产物，例如 `cargo clean -p soplex-sys -p highs-sys`，再用一致的生成器重试。本次排查仅清理了失败配置的 CMake 缓存，未清理整个 target。

## 沙箱中的 MSVC PDB 通信

切换 Ninja 后，CMake 编译器探测使用 `/Zi /FS`，在本次执行沙箱内出现 C1902 或挂起。在 `local/compiler-probe.cpp` 中仅放置：

```cpp
int main() { return 0; }
```

使用同一个 MSVC 14.52 编译器和 `/Zi /FS` 参数，沙箱外探测成功，沙箱内探测失败或挂起。随后在完整 VS 开发者环境、Ninja 和沙箱外执行完整工作区测试，SoPlex、HiGHS 与桌面 crate 均成功编译。

因此本次执行环境需要允许 MSVC 的 PDB 服务通信。没有修改项目的调试信息策略来绕过这个环境限制。

## 与普通 pwsh 构建的差异

普通 pwsh 不受本次沙箱的 PDB 通信限制，可以解释 Ninja 配置后的差异。首次生成器选择还可能受 PATH 中的 CMake、VS 工具环境及已有构建缓存影响。未读取用户那个独立 pwsh 进程的实际环境，不能断言其成功只由某一个因素造成。

本次原始失败日志、Ninja 重试、最小编译探测和最终测试日志保存在 `local/`；该目录不进入版本控制。
