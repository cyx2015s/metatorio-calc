# 本地化显示遗漏检查（2026-10-19）

检查范围：主页面、机制卡与分组、插件编辑器、选择器、流量 chip、悬停卡片、图标和前端名称缓存，以及后端目录/详情的名称传递。

## 发现与修正

| 位置 | 原有问题 | 修正 |
| --- | --- | --- |
| 项目设置的品质上限 | option 直接显示 `normal`、`rare` 等 ID | 显示品质本地化名，option value 仍是 ID |
| 选择器品质按钮 | 外层 title 使用品质 ID | 与图标提示统一显示本地化名 |
| 选择器小组标题和小组回退说明 | 直接显示 subgroup ID | 使用 `item-subgroup` 名称，支持富文本 |
| 机制分组标题 | 没有提取太阳能板、抽水地格的名称；采矿机名称只查制造机目录 | 按机制类型提取对应原型，机器名使用实体目录 |
| 无解面板 | 无供给/无消耗使用面向内部标识的 `dualVarLabel` | 显示本地化流名，同时保留品质、温度区间及燃料约束 |
| 目标、外部输入、表达式项和总流平衡 | 名称的原生悬停提示仍使用内部标签 | 使用同一套含约束的显示名称 |
| 机制流量 chip 和抽象流悬停 | 燃料、热量等部分分支回退英文枚举；污染物直接显示 ID | 统一抽象流中文名称，污染物按 `airborne-pollutant` 查名；自定义流保留用户文本 |
| 里程碑 | 电力/热量单元变体缺少显示名；太空地点被强制映射成 planet，丢失非星球地点名称 | 使用流名称；地点先查实际命名空间，再使用等价别名回退 |
| 悬停中的类别 | 燃料、插件、配方、资源类别直接输出 ID | 按对应类别查本地化名，支持富文本 |
| 星球表面属性与配方表面条件 | 自定义属性直接显示 ID | 前者查属性本地化名，后者在生成显示文案时替换属性名，数值约束不变 |
| 普通图标 | 默认 title、img alt 使用内部 ID | 使用本地化纯文本；合成电力/热量等图标也有显示名 |
| 同上下文翻译刷新 | 名称 Map 仅比较 context ID，新目录仍命中旧缓存 | 按目录对象替换重建 Map |

## 数据与回退边界

复用已有 `CatalogIndex.names`，一次下发辅助命名空间的已有翻译：`item-group`、`item-subgroup`、`fuel-category`、`module-category`、`recipe-category`、`resource-category`、`surface-property`、`airborne-pollutant`、`space-location`、`virtual-signal`。没有新增逐个查询名称的 IPC。

类别之间严格区分：例如没有 `fuel-category/chemical` 翻译时，不能拿同名 `item/chemical` 的翻译充数。只有等价命名空间允许回退，例如 module → item、mining-machine → entity、planet ↔ space-location。缓存没有提供翻译的内容仍回退 ID；本次不会凭 ID 猜译，也不保证游戏或模组为所有类别提供翻译。

有意保留的内部标识：选择器搜索结果的 ID 副标题、悬停详情的 code 行、候选项用于核对原型的 ID 提示、项目数据和 IPC 值、模组名称/版本、文件路径及程序诊断。项目、工厂、上下文和自定义流名称是用户文本，不替换成游戏翻译。

## 验证

- 前端纯函数与图标/富文本回归：`node --test tests/icons.test.mjs tests/rich-text.test.mjs tests/display-labels.test.mjs`。
- 新增真实浏览器 DOM 回归：`pnpm test:labels-browser`，使用模拟 IPC，覆盖类别、小组富文本、品质 option 值、命名空间隔离、同上下文名称更新、太空地点、抽象流及无解诊断中的品质/温度。
- 既有真实浏览器回归：`pnpm test:rich-text-browser`、`pnpm test:layout`；后者验证 1000 个机制的 72px 行高与虚拟挂载。
- Rust 回归覆盖辅助名称消息及表面条件本地化，断言原始原型约束没有被修改；常规 shell 测试跳过大规模 LP 资产枚举。
- 前端类型检查、生产构建、workspace check、shell clippy。

浏览器测试验证界面和消息消费，不等同于运行真实游戏导出或执行完整 LP 规划测试。
