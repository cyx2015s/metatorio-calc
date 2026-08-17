# -*- coding: utf-8 -*-
"""验证太阳能蓄电器配平公式。

昼夜结构（Factorio 官方默认，实测 dawn=0.75/dusk=0.25/morning=0.55/evening=0.45）：
  day（满日照）      : [0.00, 0.25] 与 [0.75, 1.00]  → perf = perf_day
  sunset（黄昏渐暗） : [0.25, 0.45]                  → perf_day → perf_night 线性
  night（夜晚）      : [0.45, 0.55]                  → perf = perf_night
  sunrise（黎明渐亮）: [0.55, 0.75]                  → perf_night → perf_day 线性

蓄电器容量 = 一天内产出超过平均稳定出力的积分（充电量），归一化峰值 P=1。
"""
import sympy as sp

t, d, n = sp.symbols("t d n", real=True)

# 分段性能曲线（峰值功率 × performance(t)）
perf = sp.Piecewise(
    (d, sp.And(t >= 0, t < sp.Rational(1, 4))),
    (d - (d - n) * (t - sp.Rational(1, 4)) / sp.Rational(1, 5),
     sp.And(t >= sp.Rational(1, 4), t < sp.Rational(9, 20))),
    (n, sp.And(t >= sp.Rational(9, 20), t < sp.Rational(11, 20))),
    (n + (d - n) * (t - sp.Rational(11, 20)) / sp.Rational(1, 5),
     sp.And(t >= sp.Rational(11, 20), t < sp.Rational(3, 4))),
    (d, sp.And(t >= sp.Rational(3, 4), t <= 1)),
)

avg = sp.integrate(perf, (t, 0, 1))
print("avg =", sp.simplify(avg))

# 找 perf = avg 的时刻（sunset 段与 sunrise 段）
crit = sp.solve(sp.Eq(d - (d - n) * (t - sp.Rational(1, 4)) / sp.Rational(1, 5), avg), t)
print("sunset perf=avg at t =", crit)
crit2 = sp.solve(sp.Eq(n + (d - n) * (t - sp.Rational(11, 20)) / sp.Rational(1, 5), avg), t)
print("sunrise perf=avg at t =", crit2)

# 充电量 = ∫(perf - avg) dt over 充电段
t1 = sp.Rational(31, 100)   # sunset 段 perf=avg（0.31）
t2 = sp.Rational(69, 100)   # sunrise 段 perf=avg（0.69）
charge = (
    sp.integrate(d - avg, (t, 0, sp.Rational(1, 4)))
    + sp.integrate(perf - avg, (t, sp.Rational(1, 4), t1))
    + sp.integrate(perf - avg, (t, t2, sp.Rational(3, 4)))
    + sp.integrate(d - avg, (t, sp.Rational(3, 4), 1))
)
print("charge =", sp.simplify(charge))
print("charge(day=1,night=0) =", float(charge.subs({d: 1, n: 0})))

# 数值验证：60kW 面板，25200 ticks = 420 s
P = 60_000.0
T = 420.0
surplus = sp.simplify(charge).subs({d: 1, n: 0}) * P * T
print(f"60kW × 420s surplus = {surplus} J = {surplus/1e6:.3f} MJ")
accu = 5e6
print(f"蓄电器 5MJ → 每面板 {surplus/accu:.3f} 个（社区标准 0.84）")

# 自定义 performance 验证：perf_day=1, perf_night=0.5
surplus2 = sp.simplify(charge).subs({d: 1, n: 0.5}) * P * T
avg2 = sp.simplify(avg).subs({d: 1, n: 0.5})
print(f"perf(1, 0.5): avg={float(avg2):.4f}×P, surplus={surplus2/1e6:.3f} MJ")
