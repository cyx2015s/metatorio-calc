# 太阳能环境与闪电估算

## 已实现的太阳能语义

当前产品约定：工厂的 `planet` 是所在空间地点，非空 `surface` 是其轨道表面类型（目前通常为 space-platform），而不是另一颗地面星球。

- 地面：读取星球的 solar-power 百分比、day-night-cycle tick；缺失属性回退 SurfaceProperty.default_value。周期只在输出秒数时除以 60。默认昼夜曲线仍为 0.7×白天性能 + 0.3×夜晚性能。
- 轨道：从 Planet 组或 SpaceLocation 组读取 SpaceLocationComponent.solar_power_in_space，除以 100 得到倍率；持续满日照，使用白天性能，无昼夜储能需求。
- 原版 dump 的 Nauvis 太空系数为 300（3 倍）、Fulgora 为 120（1.2 倍）、Vulcanus 为 600（6 倍）。对应普通 60kW 面板的轨道输出分别为 180/72/360kW。地面平均输出分别为 42/8.4/168kW。
- 地面储能仍按默认日照曲线积分求出 0.168×额定功率×周期秒数×昼夜性能差的绝对值；蓄电器容量应用该蓄电器的品质倍率。只有确实需要储能时，缺失或零容量蓄电器才会使机制不能展开。
- 轨道自动枚举只产生面板候选，不与所有蓄电器组合，也不要求上下文存在或解锁蓄电器。成本只有面板面积；旧存档里留有蓄电器选择也不计其成本。地面使用面板 + 所需蓄电器面积。
- 界面、LP 电力流和成本共用 solar_balance，避免各自套不同的平均系数。轨道设置不展示蓄电器选择，显示持续输出。工厂环境变化会刷新配平信息。
- 自定义面板的 solar_coefficient_property 不为 solar-power 时，依照原型规定改用该表面属性相对于默认值的比率，覆盖普通地面/轨道太阳能系数。

边界：昼夜曲线采用默认 dawn/dusk/morning/evening；原型 dump 不包含场景在运行期修改的这些时间点或冻结时间。零昼夜周期按持续白天处理。未增加充放电功率上限、电网拓扑和瞬态负载模型。

验证：core/runtime/shell 常规回归 237 通过、0 失败、3 忽略（未跑完整 MCP 压力矩阵），含五项新增环境测试：真实原版各星球地面/轨道数值、缺失蓄电器、品质储能、自定义系数、环境切换及无蓄电器原型时的枚举。工作区编译、相关库 Clippy、前端检查与生产构建通过；前端保留四条既有警告。浏览器回归覆盖轨道切换隐藏/恢复蓄电器控件，并保留 1000 行虚拟滚动测试。

上一轮审计把 `surface.is_some()` 列为待确认条件；本轮根据用户明确的轨道语义将其确认为产品约定。此前“周期单位不一致”具体是 GameState 字段注释写成了秒；数据本身是 tick，原来的 `/60` 不应删除。上一轮将太空系数直接视为倍率的说明也有误，现已按百分比修正。

## 闪电可以估算，但需要额外参数

官方原型已经提供了一部分必要信息：

| 来源 | 可读信息 |
| --- | --- |
| PlanetPrototype.lightning_properties / LightningProperties | 每 chunk 每 tick 的生成频率、昼夜倍率、附加表面属性倍率、闪电类型、搜索半径、目标优先级与豁免规则 |
| LightningPrototype | 每次命中的能量 |
| LightningAttractorPrototype | 捕获效率、范围扩展、内置电源缓冲等 |
| QualityPrototype / 蓄电器 | 可核对设备受品质影响的参数、储能容量 |

原版 assets 中，Fulgora 的基础频率为 `1/(60×10)` 次/chunk/tick，即在生成倍率为 1 时每 chunk 每秒 0.1 次；普通 lightning 为 1GJ，避雷针效率 0.2、集电器效率 0.4。因此一次命中在未溢出前的名义电能分别为 200MJ、400MJ。这是命中能量，不能直接当成每台建筑固定功率。

适合第一版的长期平均模型为：

```
lambda(t) = 60 * lightnings_per_chunk_per_tick * A_eff/1024 * M(t)
P_mean = mean_t(lambda(t) * E_hit * efficiency * retained_fraction)
```

其中 `A_eff` 是网络实际分得的有效捕获面积（格），1024 来自 32×32 chunk；`M(t)` 包括昼夜与表面属性倍率，`retained_fraction` 表示缓冲溢出等损失。遇到多个闪电类型，需知道或实测类型分布后对命中能量加权，不能无依据地假定均匀。

`A_eff` 无法仅由建筑数量确定：捕获范围重叠、搜索规则、品质、附近遗迹与其他更高优先级吸引源都会改变分配。不能简单用“每根杆子的圆面积 × 杆子数”。将空间切成网格/采样点、对每点按优先级分配到唯一吸引源，或用游戏实测反推网络级捕获率，会比独立逐杆计算可靠。几何形状和命中规则应先用实际游戏校准，不能只把 range_elongation 当成已知圆半径。

## 储能需要概率或仿真口径

太阳能的默认周期确定，可积分算出需要的能量差；闪电的长空档是随机的，只按长期平均功率配蓄电器无法承诺不断电。

可先用非齐次泊松过程作近似（这是建模假设，需实测检验）：区间内强度积分为 Λ 时，无命中概率为 `exp(-Λ)`。白天低/零频率带来的确定空档必须另算；活跃时段还要留随机空档的储能裕量。网络应按时间步模拟缓冲容量、充放电上限、负载和溢出，统计指定时间范围内的断电概率、未供电量与能量利用率，再按用户指定可靠度选择蓄电器数量。

若将来接入 LP，建议提供明确的“平均供能估算”参数和可靠度口径，并将容量规划置于随机校准层；不要把未经校准的闪电期望值当成确定、无限可用的免费电力。当前没有新增闪电机制或 LP 变量，本文件记录可行模型及缺少的数据。

## 来源

- [SpaceLocationPrototype.solar_power_in_space](https://lua-api.factorio.com/2.1.22/prototypes/SpaceLocationPrototype.html#solar_power_in_space)：字段定义；单位还需结合官方数据核对。
- [SolarPanelPrototype](https://lua-api.factorio.com/latest/prototypes/SolarPanelPrototype.html)：昼夜性能、自定义系数属性的优先级。
- [LightningProperties](https://lua-api.factorio.com/latest/types/LightningProperties.html)、[LightningPrototype](https://lua-api.factorio.com/latest/prototypes/LightningPrototype.html)、[LightningAttractorPrototype](https://lua-api.factorio.com/latest/prototypes/LightningAttractorPrototype.html)：闪电与收集原型字段。
- [Wube 官方星球数据](https://github.com/wube/factorio-data/blob/master/space-age/prototypes/planet/planet.lua)：太空太阳能百分比、Fulgora 闪电频率和优先级。线上内容会更新；本次数值验证使用仓库 assets 和固定 2.1.22 schema。
