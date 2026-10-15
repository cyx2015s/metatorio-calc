# 平台来源与虚拟科研

工厂地表选择 `space-platform` 后，隐式外部输入改为当前星球或太空地点轨道上的星块**物品**，品质为 normal、成本为 0。来源从该地点的 `asteroid_spawn_definitions` 出发，沿受击和死亡触发效果追踪碎裂链，再读取 `asteroid-chunk.minable` 的物品产物。不会混入地面矿点、其他地点或航线上的星岩。

外部输入面板的虚线行可直接编辑成本：提交后会保存成手动外部输入，显示实线边框，并覆盖该流的隐式免费来源。点击“设为手动”可用成本 0 固化。编辑成本沿用项目当前的时间单位。移除或撤销该手动输入后，环境来源恢复。

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
