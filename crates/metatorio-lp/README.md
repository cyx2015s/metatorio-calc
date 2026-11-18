# metatorio-lp

纯 Rust 实现的连续 `binary-f64` 线性规划精确重构与基验证引擎，不依赖 HiGHS、`good_lp` 或 C++ 工具链。

## 核心特点

- **精确有理数解释**：所有输入的 `f64` 系数按其精确的二进制有理数（`BigRational`）解释，不引入浮点容差漂移；
- **基只是提示，原模型才是证明**：浮点搜索或外部候选基仅作热启动使用，最终的原始可行性、对偶可行性、目标间隙及 Farkas 不可行证书均在完整原模型上用有理数验算；
- **求解与修复组件**：
  - `Model::solve` / `solve_dual_with_observer` / `solve_dual_with_control`：精确原始/对偶单纯形及可取消控制；
  - `float_candidate_with_observer` / `float_candidate_with_options`：带 2 的幂次缩放与 Harris 两遍比值检验的纯 Rust 浮点搜索；
  - `refine_basis_with_observer` / `repair_dual_feasible_basis` / `certify_candidate_with_observer`：精确基重构、支撑集修复与指定相对间隙预算的联合认证；
  - `prove_zero_propagation_infeasible`：基于齐次行符号传播的快速不可行预检。

## 使用与测试

```rust
use metatorio_lp::{CertifiedResult, Model, Rational};
use std::time::{Duration, Instant};

fn solve(model: &Model) -> Result<CertifiedResult, String> {
    let start = Instant::now();
    let alive = || start.elapsed() < Duration::from_secs(120);
    let proposal = model.float_candidate_with_observer(20_000, |_| alive())?;
    let budget = Rational::new(1.into(), 1_000_000_000_000u64.into());
    model.certify_candidate_with_observer(&proposal, 1_000, &budget, |_, _| alive())
}
```

```sh
cargo test -p metatorio-lp
cargo clippy -p metatorio-lp --all-targets -- -D warnings
```
