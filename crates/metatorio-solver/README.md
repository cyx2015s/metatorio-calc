# metatorio-solver

Metatorio 的线性规划建模与多后端求解调度层，无 GUI 依赖。

## 支持的求解后端

项目通过 `ProjectSettings.solver`（[`LpSolver`](src/lp_solver.rs)）选择后端，默认编译启用全部 5 个选项：

- `highs`（**HiGHS**，默认）：内置 C++ 源码编译的工业单纯形求解器，包含针对数值退化循环的有界重试阶梯；
- `clarabel`（**Clarabel**）：纯 Rust 内点法（IPM）求解器；
- `microlp`（**microlp**）：纯 Rust 轻量单纯形求解器；
- `soplex`（**SoPlex**）：通过 `vendor/soplex-sys` 补丁和 `vendor/boost` 头文件编译，写入稀疏 MPS 后启用 SoPlex 精确有理数模式（feas/opt 容差为 0）；
- `rust-certified`（界面显示 **Rust based**）：基于 `metatorio-lp` 的纯 Rust 精确重构与认证后端（为兼容旧工程，默认序列化 ID 仍保留 `rust-certified`）。

## 调度原则

- `lp.rs` 直接分发至项目所选后端，不再在适配层做二次原模型拒绝或跨后端静默回退；
- 不可行、无界、数值失败和超时由所选后端按原始语义报告；
- 若某后端未编译进当前二进制（Cargo feature 未开启），返回明确错误提示。

## 常用命令

```sh
# 仅跑纯 Rust 单纯形后端测试（无需 C++ / CMake）
cargo test -p metatorio-solver --no-default-features --features microlp,rust-lp

# 检查单个后端编译
cargo check -p metatorio-solver --no-default-features --features microlp
```
