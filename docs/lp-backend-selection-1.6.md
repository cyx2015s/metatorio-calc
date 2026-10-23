# 项目级 LP 后端选择（1.6 开发分支）

## 行为

每个项目在 `ProjectSettings.solver` 中选择一个后端：

| 值 | 后端 | 类型 |
|---|---|---|
| `clarabel` | Clarabel | 纯 Rust 内点法 |
| `highs` | HiGHS | 原生单纯形 |
| `microlp` | microlp | 纯 Rust 单纯形 |
| `rust-certified` | 旧 Rust 路径 | 当前仍保留内部证书 |

默认构建启用四个 feature。新项目使用编译进来的首选后端（有 HiGHS 时默认
`highs`，否则 Clarabel / microlp / rust-certified）；旧文档没有 `solver`
字段时按 `rust-certified` 解释。

## 不再做的事

适配层不再：

- 用原始 LP 复核返回解；
- 拒绝证书未接受的解；
- 在失败后换策略重试；
- 在自动规划回写后重建子模型做自检；
- 把写回方案放宽到 LP 支持的并集。

所选后端返回 `Ok` 就直接使用；`Infeasible`、`Unbounded`、数值错误和
超时按其原始错误返回。后端不可用（未编译对应 feature）返回明确错误。

## 构建

```powershell
# 四个后端
cargo build -p metatorio-headless

# 只编一个纯 Rust 后端
cargo build -p metatorio-headless --no-default-features --features microlp
```

## 代码入口

- `metatorio-solver/src/lp_solver.rs`：枚举、feature 可用性、默认值；
- `metatorio-solver/src/lp.rs`：四个后端的直接分发；
- `metatorio-runtime/src/document.rs`：项目设置；
- `metatorio-app/src/routes/+page.svelte`：项目设置中的后端下拉框。
