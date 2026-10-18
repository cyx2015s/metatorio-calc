# Rust 后端硬编码审计（2026-10-18）

后续太阳能修复已完成，详见 [太阳能与闪电估算](solar-and-lightning.md)：用户明确 surface 为轨道类型，因此 surface.is_some() 是当前产品约定；轨道持续满日照且无需蓄电器；太空系数按百分比换算，支持 Planet/SpaceLocation 两组；周期实际为 tick（旧注释错误，原 `/60` 正确）；缺省属性与蓄电器品质已处理。下表保留原审计时的判断，太阳能相关状态以此更新为准。

## 范围与判断标准

检查了 `crates/` 下九个 Rust crate 的源码，重点追踪参与分支、查表、原型查找、名称构造、默认值与序列化的字符串，并人工区分生产代码、测试夹具、注释示例。下表是按语义整理的关键清单，不是把每个日志句子逐条抄录，也不声称字符串搜索能证明整个领域模型正确。

依据优先为仓库固定的 `metatorio-data/schema/prototype-api.json`（2.1.22，包含继承链），其次为 `assets` 中实际 dump；后者只能证明这些上下文的数据，不能证明模组或自定义场景都遵循相同约定。当前官方文档可辅助核对，但实现以仓库版本为准。

## 1. Debug / Display / 诊断标签

| 位置 | 字符串及用途 | 结论 |
| --- | --- | --- |
| `metatorio-runtime/src/state.rs` 的 `message_label`、`RuntimeError::fmt` | 新建项目、工厂操作、未知机制等中文文本 | 历史标签与错误展示，不通过这些文字决定原型语义。 |
| `metatorio-data/src/store.rs` 的 `LoadError::fmt`、`metatorio-icons/src/render.rs` 的错误格式化 | 原型名称、组件名称与失败原因 | 输出诊断；不能把错误里的 iron-plate 等示例当成生产特判。 |
| `metatorio-solver/src/solver.rs`、`lp.rs`，`metatorio-lp/src/certified.rs` | `flow:{:?}`、`source:{:?}`、`sink:{:?}`、`balance:{:?}`、`target:{:?}`、`x{i}`、`r{i}`、搜索阶段标签 | LP 的显示名、导出标签或诊断记录；数学关联使用结构键、索引和系数，并不解析这些显示名来推断物品。 |
| 各 `#[cfg(test)]` 模块、`tests/`、`examples/` | iron-plate、steam、planet-discovery-vulcanus、quality-1 等 | 测试对象或示例；不能因出现了具体名称就认定有生产逻辑硬编码。 |

## 2. 协议、结构与版本兼容（不是名称猜测）

| 位置 | 字符串及依据 | 处理 |
| --- | --- | --- |
| `metatorio-data-codegen/src/config.rs`、`type_map.rs`、`emit.rs` | schema 类型名、组件名、字段名、Rust 基础类型名、关注/忽略列表 | 编译期映射；加字段应修改这里或已有自定义类型，不能把生成文件当源码手改。 |
| `metatorio-data/src/types.rs`、`adapt.rs` | item/fluid、energy_source、category/categories、probability/independent_probability 等 | 原型协议及历史 dump 形状。适配按字段结构与继承组件匹配，不按具体物品名补丁。 |
| `metatorio-runtime/src/message.rs`、`migrate.rs` | new-project、recipe、factorio:recipe、schema_version 等 | 应用消息与历史存档协议；旧标签必须保持可迁移。 |
| `metatorio-shell/src/app.rs`、`mcp.rs`、`prototype_calculation.rs` | catalog kind、document-changed、auto_plan、created、crafting-speed 等 | IPC/MCP 与计算 DTO 的稳定键；修改会影响前端或客户端。不是物品、科技命名约定。 |
| `metatorio-icons/src/sources.rs`、`render.rs` | base/core、`__mod__/...`、data、space-location 等 | 游戏资源寻址与应用图标目录约定。已枚举 data 的 DLC 子目录，没有把所有 DLC 限死成 space-age。 |
| `metatorio-solver/src/lp.rs`、headless 的命令行配置 | rust-certified、rust-exact、后端名、环境变量和 CLI 选项名 | 有些字符串确实参与算法选择，不能归为纯 Display；它们是应用配置协议，不是在猜游戏原型语义。 |

## 3. 有 schema 或明确领域依据的常量

| 字符串 | 位置 / 依据 | 结论 |
| --- | --- | --- |
| normal | `metatorio-core/src/id.rs`、`context.rs`、`accessibility.rs`、data 的品质链 | 官方 `QualityPrototype.name` 明确区分 normal/quality-unknown。正常品质默认值有领域依据；实际升级链读取 `next`，并未硬编码五种品质名称。 |
| crafting | data `ext.rs`、runtime `prototype.rs` | `RecipePrototype.categories` 的 schema 默认值。 |
| basic-solid | runtime `prototype.rs`、`auto_plan.rs` | `ResourceEntityPrototype.category` 的 schema 默认值。 |
| chemical | data `types.rs` | `BurnerEnergySource.fuel_categories` 的 schema 默认值，不代表所有燃料都必须是 chemical。 |
| heat-fluid-inside / output-to-separate-pipe | data `BoilerMode`、core 展开与可达性 | 官方锅炉模式的枚举字面值；水和蒸汽由流体盒过滤器推导，没有按 water/steam 名称写生产特判。 |
| unlock-recipe / unlock-quality / unlock-space-location | data `Modifier`，core 解锁图 | 科技效果的协议字面值；目标来自对应字段，而不是科技名。 |
| solar-power / day-night-cycle | runtime `apply_environment_to_game_state`，assets 的 surface-property | 对应已存在的表面属性 ID。名字有数据依据，但当前读取路径、默认值及时间单位仍有模型问题，见下一节。 |

## 4. 推测、应用默认或尚有适用边界的内容

不能从代码反推作者当时是否“猜的”，这里按当前证据判断，不用“作者知道领域”替代来源。

| 内容 | 判断与影响 | 本轮处理 |
| --- | --- | --- |
| `planet-discovery-{name}` | **错误的命名推测。** 即使在原版常见，也不等于任意模组科技名规则。 | 已删除。Planet 和 Space 统一读取 `unlock-space-location.space_location`，多个科技解锁同一地点为 OR。里程碑排序、自动可达性、资源依赖共用此图。 |
| 配方/品质/地点解锁表只以 `String` 为键 | **隐含的跨类别名称唯一假设。** 不同原型类别允许同名，会误借另一类别的解锁科技。 | 已改为 `HashMap<Accessible, Vec<String>>`，目标带类别，地点效果按实际 Planet/Space 分类。 |
| `SpacePlatforms` 自动 kebab-case 成 `space-platforms` | **与 schema 不一致。** 实际标签是 `unlock-space-platforms`，旧实现会静默落到 Other。 | 添加明确 serde 标签。保留继承自 BoolModifier 的必填 modifier 字段；没有给缺失字段猜默认值。 |
| `nauvis` 初始可达；新工厂默认 Nauvis | **应用的默认起点策略，非任意场景定律。** 原型 dump 不包含玩家 force 的解锁状态或场景选择的出生地点。 | 保留默认体验并明确注释；若连 Nauvis 都有显式解锁效果，效果优先。自定义起点仍需里程碑强制覆盖；未增加一个凭名称猜测起点的规则。 |
| `surface == "space-platform"`；平台初始 `empty-space`；启动包未指定 surface 时回退 space-platform | **原版数据与现有应用约定，通用性未证实。** assets 确有这些原型，但 schema 没有把任意自定义 surface 判为平台的 boolean 字段。 | 本轮未扩展平台身份模型。自定义启动包 surface 等场景需要后续明确数据契约；不能改成 gravity=0 就是平台。 |
| 星球与 Surface 同名就优先使用这个 Surface | **名称关联启发式。** 当前 schema 没有声明这种同名外键；assets 也只有平台 Surface。 | 记录为风险，未将此约定包装成引擎保证。显式选择的 surface 本身是可靠引用。 |
| `apply_environment_to_game_state` 用 surface.is_some() 判太空，轨道只查询 SpaceLocation 组 | **另一个环境简化。** 任意 Surface 不必是平台，Planet 还可能在独立组中，导致漏读轨道太阳能系数。 | 记录为后续修复点；不能靠更换某个硬编码星球名解决。 |
| solar-power 默认 1 倍、昼夜周期 25200、平均日照 0.7、储能系数 0.168 | **模型默认与原版曲线近似，非通用原型真理。** 此项不是字符串但会影响计算；GameState 周期注释写秒，而 solar_balance 除以 60，且 surface-property 默认值未统一回落。 | 单列为待核对的太阳能模型问题；未在解锁修复里顺手改变计算口径。 |
| 图标 ScaleLaw 中的倍数 / 默认 scale | **有对照实验依据的渲染模型。** 影响图标而非 LP/可达性；比“按物品后缀猜语义”不同，但也不等于完整引擎还原。 | 保留，继续以真实渲染对照检验，不将注释里的 recycling 示例视为配方生产特判。 |

## 类型与生成器

`Modifier::UnlockSpaceLocation` 和 `UnlockSpaceLocationModifier.space_location` 已存在；生成器也早已通过 `custom_type_map` 将 `Modifier` 映射到手写 ADT。因此本轮无需新增字段或大范围生成类型，错误位于消费者和一个 serde 标签。新增测试直接对照固定 schema 的 SpacePlatformsModifier 标签及 BoolModifier 继承字段，防止再按 Rust 变体名推导错协议标签。

`Other` 仍用于吸收尚未建模的效果。修正平台效果标签只保证正确读取，不表示本轮新增了平台或流体采矿的布尔科技门槛模型；这与地点解锁图修复是两个范围。

## 验证

- 使用任意科技名解锁星球/深空地点，验证里程碑锁定及排序。
- 验证“看似标准发现科技名但没有效果”不能解锁星球。
- 验证多科技任意一个可达即可解锁，以及配方/品质/地点同名互不串线。
- 在真实原版 dump 中重命名 Vulcanus 发现科技并修复其引用，可达性保持；锁定重命名后的科技则阻断该星球。
- 验证 Nauvis 上声明的解锁效果优先于应用默认起点。
- 验证 unlock-space-platforms 的解析、序列化及 schema 标签，保留 modifier 的 true/false 语义。

本轮实际执行：data-codegen/data/core/runtime/shell 常规回归 313 通过、0 失败、4 忽略（跳过大规模 MCP auto_plan 资产矩阵）；新增 schema 守卫后单独复跑两项 Modifier 测试通过。真实资产 MCP `base_iron` 规划与写回测试通过，工作区编译和相关库 Clippy（`-D warnings`）通过。未运行完整 LP 压力矩阵，也未把上表的待核对太阳能/平台模型问题视为已修复。

外部核对：[UnlockSpaceLocationModifier](https://lua-api.factorio.com/latest/types/UnlockSpaceLocationModifier.html)、[TechnologyModifier](https://lua-api.factorio.com/latest/concepts/TechnologyModifier.html)。这些在线文档可能更新；固定版本证据是仓库的 2.1.22 schema。
