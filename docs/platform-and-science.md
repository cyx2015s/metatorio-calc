# 平台来源与虚拟科研

工厂地表选择 `space-platform` 后，隐式外部输入改为当前星球或太空地点轨道上的星块**物品**，品质为 normal、成本为 0。来源从该地点的 `asteroid_spawn_definitions` 出发，沿受击和死亡触发效果追踪碎裂链，再读取 `asteroid-chunk.minable` 的物品产物。不会混入地面矿点、其他地点或航线上的星岩。

外部输入面板的虚线行可直接编辑成本：提交后会保存成手动外部输入，显示实线边框，并覆盖该流的隐式免费来源。点击“设为手动”可用成本 0 固化。编辑成本沿用项目当前的时间单位。移除或撤销该手动输入后，环境来源恢复。

## 自动枚举的环境边界

星球表示平台所在的轨道地点，并不代表工厂在该星球地面。选择 `space-platform` 时，不自动枚举地面采矿和种植；普通低重力或真空星球不受此规则影响。

地面和太空使用同一套地格可达性推导：地面从星球生成表出发，平台从 `empty-space` 和匹配地表的启动包初始地格出发，再按物品的 `place_as_tile` 递归求闭包。铺设检查碰撞层、`invert`、`tile_condition` 和 `allows_being_covered`，只在所得集合中枚举流体抽取和物品销毁。没有对应地格就没有对应机制。此处只描述地形存在的可能性，不模拟布局和铺地物料成本。

配方与机器、资源与矿机、种植物、能量设备、太阳能板与蓄电器均按原型的表面条件过滤。制造机和矿机在评分、截取候选名额前检查可达性，未解锁的高速设备不会挤掉已解锁的慢速设备，也不会作为兜底重新引入。插件塔方案同样要求塔存在、已解锁且可在当前表面建造。

火箭发射产物要求上下文中有可建造、已解锁的发射井；模组若允许平台发射，仍按原型条件保留。变质、物品/流体燃料、流体热量及虚拟科研转换不依赖地面地格。

上述边界针对自动枚举。显式启用“保留现有机制”仍允许已有手动配置作为环境例外参与规划。

## 虚拟科研

- 流：`{"VirtualScience":"automation-science-pack"}`，只保存物品原型名，不带品质。
- 机制：`{"type":"virtual-science","item":{"id":"automation-science-pack","quality":"legendary"}}`。
- 消耗 1 个指定品质的科研包，产出该包对应的科研价值，转换机制成本为 0。
- 普通 item 的价值为 `science_capacity × quality.science_capacity_multiplier`。
- tool 原型使用 `durability × quality.tool_durability_multiplier`。
- `science_capacity_multiplier` 默认继承 `tool_durability_multiplier`；后者默认 `1 + level`。标准品质对应 1、2、3、4、6 倍。

科研包由实验室 inputs 或科技 unit.ingredients 识别，支持模组物品名。不同品质汇入同名虚拟科研，不同科研包之间不互换。页面的目标选择器有“虚拟科研”页签，也可手动添加“虚拟科研”机制；自动规划会枚举可用品质的转换。

这是科研包容量的折算，不包含实验室速度、实验室品质减耗或生产力。

字段依据仓库内 2.1.22 `prototype-api.json`，参见官方 [QualityPrototype](https://lua-api.factorio.com/latest/prototypes/QualityPrototype.html)、[ItemPrototype](https://lua-api.factorio.com/latest/prototypes/ItemPrototype.html) 和 [ToolPrototype](https://lua-api.factorio.com/latest/prototypes/ToolPrototype.html)。
