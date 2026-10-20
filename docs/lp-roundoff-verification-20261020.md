# 从浮点残差判断零与非零（2026-10-20）

## 能证明什么

给定计算值 r̂ 和经过证明的误差上界 E，真实表达式值位于 [r̂−E,r̂+E]。区间不包含零时，可以证明非零；包含零时只能说尚不能确定，不能据此证明精确为零。只有区间退化到零、符号恒等式、精确算术或额外离散性条件，才能完成零的证明。

需要区分两类误差：

1. 对给定的二进制浮点 A、x、b 计算 Ax−b 时的运算舍入。补偿点积、误差无损变换、浮点展开、超累加器或有理数点积可以消除或严格包住这类误差。
2. A、x、b 相对于建模意图或真实解本身的误差。精确计算一个近似 x 的残差并不能消除 x 的误差，更不能由这个点不满足约束推断原问题无解。

若仅 x 有已知逐分量界 |x−x̂|≤e，且 A、b 精确，则残差的数据误差界为 |A|e；再加上点积计算的严格舍入界。A、b 也有误差时，需要相应项及乘积交叉项。必须保留相关性：把同一变量的多次出现当成独立区间，会显著放大区间。

## 与当前 LP 的关系

当前路径用精确二进制有理数复核全模型，因此已排除“检查点积自己发生舍入”的问题。观测到约 1e−16 的残差，是该候选点相对于所存二进制模型的真实非零残差；其来源可能是系数生成或候选解近似，不能自动宣称是精确零。

用户授权的 1e−12 逐行相对容差，是工程验收定义，与舍入误差证明是两件事。容差路径同时检查精确候选与实际 f64 输出，不设置绝对误差下限。精确模式仍检查原模型的精确零。

精确零可利用基 B：验证 B 非奇异后证明 x_B=B⁻¹b 的存在及各分量范围；若非基变量取边界，且所有基本变量满足界，就能证明相应等式系统存在精确可行点。退化点的分量区间跨零时仍需更高精度或精确处理；非基本行、变量界和对偶约束必须继续检查。区间方法不是保证一次成功的替代品。

大量变量本身不妨碍精确可行性；真正困难的是严重相消、病态基、退化、生成系数的二进制近似，以及返回的 f64 通常不能表示精确有理解。

## 直接相关论文

- Ogita, Rump, Oishi (2005), [Accurate Sum and Dot Product](https://ogilab.w.waseda.jp/ogita/math/doc/2005_OgRuOi.pdf), SIAM J. Scientific Computing 26(6):1955–1988, DOI 10.1137/030601818。误差无损变换、高精度补偿求和/点积及可验证误差界。实现需处理溢出、下溢和具体求值顺序；Kahan 求和本身不能提供精确零证书。
- Shewchuk (1997), [Adaptive Precision Floating-Point Arithmetic and Fast Robust Geometric Predicates](https://www.cs.cmu.edu/~quake/robust.html), Discrete & Computational Geometry 18:305–363。先用快速计算和误差界筛选，仅对无法可靠判定的表达式增加精度或精确展开。几何谓词可迁移的是验证思想，不能直接把几何代码当 LP 求解器。
- Jansson (2004), [Rigorous Lower and Upper Bounds in Linear Programming](https://tore.tuhh.de/entities/publication/54a09f8e-7fb5-4841-a492-145d986334c4), SIAM J. Optimization 14(3):914–935, DOI 10.1137/S1052623402416839。直接研究 LP 的严格目标上下界及区间输入，不等同于只看残差大小。
- Rump (2010), [Verification Methods: Rigorous Results Using Floating-Point Arithmetic](https://www.tuhh.de/ti3/rump/intlab/ActaNumerica2010.pdf), Acta Numerica 19:287–449。严格线性系统包含、区间验证及 LP 验证综述；LP 部分第14.1节。
- Gleixner, Steffy, Wolter (2016), [Iterative Refinement for Linear Programming](https://pubsonline.informs.org/doi/abs/10.1287/ijoc.2016.0692), INFORMS J. Computing 28(3):449–464。使用浮点 LP 求解修正问题、精确计算和维护残差，可逐步获得高精度或精确解。

## 后续工程优先级（尚未实现）

先尝试以严格误差界的点积作为快速过滤器，在界不能决定结果时调用现有精确点积；测量其能否降低全模型验收成本。再研究对候选基的区间线性系统验证及按需精度提升。不要仅因区间包含零就接受精确零，也不要把相对容差点与对偶下界的接近误称为原 LP 的严格最优性证书。
