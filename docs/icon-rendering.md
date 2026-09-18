# 图标渲染：为什么自己做、怎么做、怎么验证

## 背景

上下文（游戏数据）的导出流程原本是：调用 `factorio.exe --dump-data`（原型数据）、
`--dump-prototype-locale`（翻译）、`--dump-icon-sprites`（图标贴图），再把 `script-output`
搬进缓存目录。前两步无头可用；**图标这一步在 Steam 版会弹「是否启动游戏」的确认**，
自动流程会被这一下打断。

图标本身并没有那么神秘：原型数据里带图标的原型都带 `icon` / `icons`
（`IconData` 层数组），没带的按官方写明的规则推导（见事实 10），要做的只是「读 PNG →
按规则叠加」。因此改为**在 metatorio 自己的进程内渲染**：导出只剩 `--dump-data` 与
`--dump-prototype-locale` 两条无头命令，`--dump-icon-sprites` 已经不再调用。

实现落在新 crate **`crates/metatorio-icons`**：

| 模块 | 职责 |
| --- | --- |
| `sources` | 把 `__base__/…`、`__core__/…`、`__mod__/…` 解析到真实文件：`<游戏>/data/*` 各目录（含 DLC 的 `space-age`）、mod 目录（解压目录或 `<名字>_<版本>.zip`，按需只读一个条目） |
| `image` | RGBA8 缓冲、PNG 解码/编码（调色板/灰度/16 位归一化）、直通↔预乘、面积平均重采样、source-over 合成 |
| `render` | 按 `IconData` 规则把一层或多层叠成一张图标（`render_icon` / `render_record_icon`）；没有图标定义时按官方写明的规则推导（配方 → 主产物/唯一产物）；`render_all_icons` 遍历整个原型仓库，把结果写进缓存目录 |
| `compare` | 与官方导出逐像素比对（统计与验收） |

验收工具：`examples/compare.rs`

```text
cargo run -p metatorio-icons --example compare -- \
  --context "<contexts>/<id>" --game "D:\异星工厂\Factorio_2.1" [--mods <mod 目录>] \
  [--type item] [--limit 200] [--tolerance 2] [--types] [--save <目录>] \
  [--show <type>/<name> --pixels 36,2;40,2] [--sweep] [--sweep-multiplier] [--sweep-kernel] \
  [--fit-scale] [--sheet <对照图.png> [--sheet-count 12]] [--render-only <目录>] \
  [--check-canvas] [--canvas-sweep] [--normalize]
```

`--sheet` 是**给人眼看的**：把匹配率最差的若干张拼成「左=我们 / 右=官方、白底=透明」的
对照图——数字过关不等于看着像（`scale` 口径那条就是这么发现的），反过来也一样。
`--render-only` 只跑应用注册上下文时的那一步并计时（不读参考图），用来估时间。
`--check-canvas` / `--canvas-sweep` 只用官方参考图的**尺寸**（读 PNG 头，不解码）验画布口径，
几千张也是秒级。`--normalize` 在两边尺寸不同时先把大的缩到小的再比——比的是**构图**而不是
分辨率；实测影响很小（py 83.94% → 84.22%），说明那批尺寸不符的图标差异不只是分辨率。

**性能实测**（release / debug 都测过，同一台机器、同一份 dump；`--render-only` 只跑应用
注册上下文那一步，含解码/缩放/编码/写盘）：

| 上下文 | 图标数 | debug | release |
| --- | --- | --- | --- |
| vanilla | 2134 | 20.0s（9.4 ms/张） | **2.5s（1.2 ms/张，841 张/秒）** |
| py | 16493 | ——（按 debug 比例约 40 分钟） | **19.6s（1.2 ms/张，842 张/秒）** |

- release 比 debug 快 **约 8.7 倍**——**调参/验收一律用 `--release`**，否则一个全量比对
  就是几十分钟起步。
- 但**光靠 release 不够**：py 一开始在 release 下要 **455.8s（27.6 ms/张）**，是 vanilla 的
  25 倍，而两者每张的像素量差不了那么多。查下来是**每读一张图都重新打开一遍 mod 的 zip**
  （py 的 graphics 包单个上百 MB，重开一次解析中央目录就要几毫秒）＋ 同一张贴图被几千个
  原型重复解码（`recycling.png` 尤其明显）。加了「zip 只开一次 + 解码结果按资源路径缓存
  （128 MB 封顶）」之后：**455.8s → 19.6s（23 倍）**，每张成本掉到和 vanilla 一样的
  1.2 ms。这两个缓存都在 `IconSources` 里，语义不变、只是省时间。

输出分四块，**四块都算数**：参与比对的（按参考图目录汇总，`--types` 再按原型类型展开）、
官方有图但 dump 里没有图标定义（游戏自动生成，未实现）、官方有图但原型仓库里没有这条原型
（类型不在关注列表）、以及「同名图有多份、只用了按原型 `type` 优先的那份」。跳过的东西
必须被数出来，否则「没报错」会被误读成「都对」。

## 接进应用

注册上下文时（`metatorio-app` 的 `register_context_files` / `register_context_and_activate`）：

- 导出只剩 `--dump-data`、`--dump-prototype-locale`；图标来源由枚举 `IconSource` 表达：
  `Render { game_root, mod_dir }`（游戏导出 → 自己渲染）、`Copy(dir)`（外部已备好的贴图
  目录，例如用户自己用游戏导出的那套）、`None`（内嵌 dump / 用户自备 dump，没有游戏目录可读）。
- 新上下文（或历史缓存缺图标目录）时，`IconSource::Render` 先建出 `icons/` 占位目录，
  再在**阻塞线程池**里跑 `render_icons_into`——它只吃 dump 字节与目录路径、不碰 runtime，
  于是既可以丢进 `spawn_blocking`，也能在测试里直接调用。写盘布局为
  `icons/<type>/<name>.png`，与 `icon` 命令的读取路径一致。
- 图标是 best-effort：渲染失败**不挡**上下文注册，但必须如实报出来，并把空的 `icons/`
  目录收掉——`icon_root` 不存在 = 前端退回占位图标，不留「有目录却没图」的假象。
- **上下文元数据只记「游戏版本 + 启用的 mod（名字 + 版本）」**：路径（可执行文件 / mod 目录）
  不进元数据——它只在导出那一刻有用（图标当场渲染完），旧缓存里的 `source` 字段被忽略。
- 渲染走 `render_record_icon_with`：自带 `icon`/`icons` 的按定义画；没有的按官方写明的
  **推导**画（配方 → 主产物/唯一产物）。
- 完成情况打到 stderr：`图标渲染完成：写出 N 张（其中按官方规则推导 D 张；<type> N、…），
  无图标定义 X、推导失败 W、缺文件 Y、解码失败 Z`——推导出来的和画不出来的都不能省。
- 测试 `context_registration_renders_icons_without_running_the_game` 串起「注册 → 渲染 →
  读回 PNG」整条链（本机没有 `<游戏>/data/base` 时打印 `[skip]` 跳过）。

## 非原型图标（`utility-sprites`）

界面上还有一类图标**不属于任何原型**：组装机选燃料按钮、插件/弹药/装甲/机器人槽的**空槽背景**、
`fuel_icon`、`ammo_icon` 这些。它们在 dump 的 **`utility-sprites`** 节点里
（形状是 `utility-sprites` → 原型 `default` → 575 个具名字段），不在原型体系里。

实现：`crates/metatorio-icons/src/utility.rs`

- 解析每个字段：`filename` + 尺寸（`size: 64` 或 `size: [w, h]`，也接受 `width`/`height`）+
  位置（`x`/`y` 或 `position: [x, y]`）+ 可选 `scale`/`tint`；
- 数组形态（SpriteVariations）取第一个；`layers`、`stripes`（动画帧）、`cursor_box` 这类
  **不猜、如实计数**；
- 输出到 `<图标目录>/utility/<字段名>.png`，和应用里 `icon` 命令的读取路径一致，
  前端用 `type: "utility"` 请求即可（`empty_module_slot`、`empty_ammo_slot`、
  `empty_gun_slot`、`empty_armor_slot`、`empty_robot_slot`、`empty_trash_slot`、
  `empty_drop_cargo_slot`、`fuel_icon` …）。

实测覆盖（两个上下文一致）：**解析出并写出 569 张，缺文件 0、解码失败 0，形态不支持 6 个**
（`achievement_label` 三个、`arrow_button`、`cursor_box`、`platform_entity_build_animations`
——它们是多层/逐帧的 GUI 部件，不是图标）。小 dump（没有 `utility-sprites`）时是「没得画」，
不是失败。

**验证方式**：游戏的 `--dump-icon-sprites` 只导出原型图标，**没有官方参考图可比对**，所以这类
图标的验证是「按定义裁切」＋人眼看（`examples/compare.rs --utility <目录>` 渲染完自己打开图）。
已看过 `empty_module_slot.png`（灰色空槽底 + 三个圆点 + 一条横条）与 `fuel_icon.png`
（红色燃料警告三角）——与游戏里的一致。

注册上下文时（`render_icons_into`）会顺带把这类图标一起写盘，并在 stderr 打一行
`非原型图标（utility-sprites）：写出 N 张，缺文件 X、解码失败 Y、形态不支持 Z`。

## 已经查清的事实（都是实测，不是猜的）

1. **游戏导出的图标是「预乘 alpha」**，源 PNG 是**直通 alpha**。
   实测：导出像素 ≈ 源像素 RGB × A/255（例如源 `(159,159,159,44)` → 导出 `(27,27,27,44)`）。
   所以比对时要把我们自己渲染的直通结果**预乘一次**再比（`Rgba8::premultiplied`）；
   直接比原始值会把「预乘」误判成大面积差异——这正是最初 4 个图标「看起来一样、
   像素差一大截」的原因。
2. **图标文件是「mipmap 横排」**：`graphics/icons/iron-plate.png` 是 120×64 =
   64+32+16+8，level 0 就是左上角的 64×64。渲染取这一块即可。
3. **画布尺寸的口径**（这条改过两次，都是被实测逼出来的，最后一条**采用**）：
   - 只有 `icon`（老式单层图标）时，画布 = **原型的 `icon_size`**，没给就按类型默认
     （物品/实体/流体 64，科技 256，成就/物品组 128）；
   - 给了 `icons` 时，**原型的 `icon_size` 根本不参与**（官方文档原文就是 "Only loaded if
     `icons` is not defined"）。实测三例（py 配方）：原型 `icon_size = 32` + 层
     `[256, 32]` → 官方 **256×256**；层 `[64, 32]` → **64×64**；层 `[1, 32]`
     （pyvoid 占位底层）→ 官方就是 **1×1**。一开始按原型的 32 去画，整个 py 上下文的
     配方匹配率被钉在 53.7%。
   - **多层时画布 = 所有层绘制矩形（含 `shift`）的并集包围盒，绘制边长向下取整**（采用）。
     验算：`space-connection` 三层（64、64×0.333 shift(-6,-6)、64×0.333 shift(6,6)，按
     「×2」口径得 64 / 42）→ x/y 并集 `[-33,33]` = **66×66**，官方正是 66×66；
     py 的 `Phadai-…-2-dubstep` → x 66 / y 68，官方正是 66×68。拿官方参考图的**尺寸**当
     标准（`--check-canvas`，只读 PNG 头）：**并集（向下取整）vanilla 1885/1885 = 100%、
     py 12942/13213 = 97.9%**；并集（四舍五入）99.5% / 97.9%；「第 0 层层边长」只有
     93.0% / 72.1%。
   - 官方文档里 512 只针对 `SpaceLocationPrototype::starmap_icon`、32 只针对
     `ShortcutPrototype::small_icons`——**普通 `icons` 不能套这两个数**（一开始把
     `space-location` 设成 512，`space-location`/`space-connection` 的匹配率掉到 33%，
     因为拿 512 画布去和官方 64 画布比，只比到了中心那一小块）。
   - **代价要说清楚**：并集口径把整套构图按原生分辨率完整画出来、不裁掉溢出部分；但官方
     导出**有时只写一份更小的**（`recipe/empty-acetylene-canister` 官方 35×35，按并集算是
     70×70，正好一半），于是**逐像素匹配率反而低一点**（py 87.0% → 83.9%；
     vanilla 92.37% → 92.34%）。看 `--sheet` 对照图我们的更完整、更清楚，所以**采用并集**；
     逐像素更高的是「第 0 层层边长」那条老口径，它把溢出部分裁掉了——数值好看但图不完整。
     画布尺寸分布（并集口径）：vanilla 平均边长 91.3、>128 的 277 张（多为 256 的科技）、
     >256 的 120 张、最大 292；py 平均 64.8、>128 的 225 张、>256 的 110 张。
4. **`space-age`（DLC）不是 mod**：它在 `<游戏>/data/space-age`，所以路径解析要把
   `data/` 下**每个子目录**都注册成根（否则 54/200 个物品图标直接找不到文件）。
5. **图标里有调色板（Indexed）PNG**：解码要开 `png` 的 `EXPAND`（顺手也做了 16→8 位）。
6. **官方导出不画 `draw_background` 描边**：文档说首层默认画描边/阴影，但实测导出
   ≈ 源图预乘（单层图标逐像素吻合），所以本实现不画。
   （注：2.1.14 的 schema 里 `IconData` 有 `draw_background`，但当前 codegen 没生成该字段；
   真要用得先在 `metatorio-data-codegen` 的字段规则里放行。）
7. **`shift` 的单位**：官方文档说「整体图标被假定为 `expected_icon_size / 2` 像素宽高」，
   即 `{0, expected/2}` 表示整整挪一个图标高度 ⇒ 1 单位 = 2 像素；实测
   `lane-splitter` 的叠加层中心与官方**完全一致**，该口径可用。
8. **`scale` 口径**（**逐像素定标过两轮，最终结论见下**）：
   - 第一轮（只看候选公式）里 `expected × scale` 略好于 `层边长 × scale`，但两者都只有
     90% 上下；
   - 第二轮改成**直接扫「显式 scale 的倍数」**（`--sweep-multiplier`，只统计有显式 scale
     的 463 张原型）：×1 70.21%、×1.5 70.81%、×1.75 71.86%、×1.875 73.45%、
     **×2 74.40%**、×2.25 66.75%、×2.5 60.28%、×3 49.03%——**×2 明显是最优点**；
   - ×2 正好对上官方文档那句默认值 `scale.unwrap_or((expected/2)/icon_size)`：**文档公式
     整体还要乘 2**。合起来就是当前采用的 [`ScaleLaw::DoubledDocDefault`]：
     `2 × 层边长 × scale.unwrap_or(expected/(2×层边长))`——没给 `scale` 时结果正好是画布
     边长（所以单层图标不受影响），给了 `scale` 时是「层边长 × scale × 2」。
   - 效果（vanilla，`--tolerance 2`）：总体 91.5% → **92.4%**；`technology` 97.4% →
     **99.2%**、`quality` 92.8% → **96.5%**、`recipe` 79.8% → **81.8%**、
     `space-connection` 33.1% → 36.8%。
   - **第三轮：追 py 那 265 张「画布尺寸对不上」的**（`--canvas-sweep`，把「尺寸倍数」与
     「`shift` 单位」拆开扫）。结论：`×2 / 1 单位 = 2 像素` 在**两个上下文里都是最优**
     （vanilla 尺寸 100%、py 97.95%；`×1` 两个上下文都近乎全错——单层图标会被画成一半）。
     py 那 265 张的尺寸比例不集中在 1/2（`≈1/2` 71 张、`0.64` 119 张、`0.68` 49 张……），
     看着像「官方在这类图标上写了更小的一份」。顺着这个猜了个口径
     `factor = (expected/32).clamp(1, 2)`（32 画布 → ×1、64 及以上 → ×2）：
     **尺寸一致率确实涨了（py 97.95% → 98.55%，vanilla 保持 100%），但逐像素反而掉**
     （py 81.00% → **78.51%**，vanilla 持平 91.66%）——同一批图标「外接尺寸像 ×1、内容
     像素像 ×2」，说明官方在这类图标上本身就不自洽（或者还有别的因子没找到）。
     所以**保持 ×2**，那条口径留作候选（[`ScaleLaw::ExpectedUnit`]），工具留作证据。
   - 这条是**看图看出来的**：先把「最差 12 张」拼成「左=我们 / 右=官方」的对照图
     （`--sheet`），官方那侧缩放层明显更大一截，「包围盒反推」又不可靠（多层叠在一起），
     于是改成直接扫倍数。
   - **重采样核**（`--sweep-kernel`：面积平均 vs 「先 2×2 降 mip 级别再双线性」）在 vanilla
     上**完全没区别**（两种都是 91.46% / 44 张全等）——2 的幂次缩放时两者本来就等价，
     vanilla 的非整数倍缩放层太少。默认保持面积平均。
9. **官方导出的目录名不等于原型 `type`**（这是比对工具最初只有 37% 覆盖率的原因）：
   所有**实体类型**（`assembling-machine`、`tree`、`explosion`、`corpse`、`simple-entity`、
   `resource`、`container`、`inserter` …）写在 `entity/` 下，所有**物品子类型**
   （`ammo`、`gun`、`module`、`armor`、`capsule`、`item-with-entity-data` …）写在 `item/`
   下，`planet` 写在 `space-location/` 下，其余类型才是 `<type>/`。所以比对工具改成
   **按文件名建索引**，解析时优先找「该原型实际会落到的目录」——这个目录由**聚合组**
   推出来（`Item` → `item/`、`Entity` → `entity/`、其余用自己的 `type`），不是硬编码名字，
   也把实际用到的映射打印出来（规律是实测出来的，不是假设的）。修正后覆盖率 1027 → 1885 张。
   只看名字会踩同名：`module/fish` 被解析到 `entity/fish.png`，实测只有 2.86%；按聚合组
   落到 `item/fish.png` 才对。
10. **官方只写明了两种图标推导**（把官方 schema `crates/metatorio-data/schema/prototype-api.json`
    里所有带 `icon` 的 description 都过了一遍，只有这两处写了「否则用……」）：
    - **配方**：没有 `icons`/`icon` 时用 `main_product` 或**唯一产物**的图标；「多产物且没有
      `main_product`」或「没有产物」时官方要求**必须显式给 `icon`**，也就是没有推导；
    - **地块**：没有 `icons`/`icon` 时用 `variants.material_background` 当图标（**还没实现**）。
    `item → place_result` 这条**官方文档里没有**：`ItemPrototype::icon` 写的是
    "Only loaded, and mandatory if `icons` is not defined"，实测两个上下文里也**没有任何
    物品缺图标定义**（vanilla + py 的「无图标定义」清单里一个 item 都没有），所以不实现。
    其余类型（`item-subgroup`、`recipe-category`、`module-category`、`resource-category`、
    `fuel-category`、`autoplace-control`、`surface-property`、`projectile` …）在 dump 里就是
    光秃秃的 `{type, name}`，官方**没有任何推导说明**，但 `--dump-icon-sprites` 里却有图
    ——游戏自己按未公开的规则拼的，我们保持「不猜、如实计数」。
11. **有两个类型的 `icon` 不是文件名而是 `Sprite`**：`airborne-pollutant`、
    `burner-usage`（本地官方文档里 `icon :: Sprite`，dump 里是 `{"filename": …}`）。
    我们的 `IconComponent.icon: Option<String>` 解析不了它们，于是被误判成「没有图标定义」
    ——这是**数据层的坑**，不是推导规则问题。

## 还差什么（已知偏差）

比对**必须把几类账都摆出来**，跳过的东西不数出来就等于假装没问题。vanilla
（`2d1e8c21a400155c`）与 py（`c3544821b3232cf9`）两个上下文：

| 类别 | vanilla | py | 说明 |
| --- | --- | --- | --- |
| 参与比对 | 2322 | 16797 | 官方导出里有参考图的原型 |
| 其中靠**推导**画出 | 249 | 3280 | 配方 → 主产物/唯一产物 |
| 其中推导也画不出 | 188 | 304 | 官方有图、但类型没有推导规则（地块、`*-category`、`projectile` …） |
| 渲染失败（缺文件/解码） | 0 | 0 | |
| 无图标定义、官方也没图 | 428 | 584 | 一致，没什么可画 |
| 官方有图、但仓库里没有这条原型 | 429 | 300 | `decorative` 163、`virtual-signal` 155、`achievement` 85、`shortcut` 20 …（类型不在关注列表） |
| 同名图在导出里有多份 | 24 | 64 | 只用了「按聚合组该落到的目录」那一份 |

按参考图目录（vanilla `--tolerance 2`）：

| 参考图目录 | 张数 | 平均像素匹配率 |
| --- | --- | --- |
| entity（爆炸 225、尸体 177、树 32、机械 …） | 766 | 97.4% |
| item（含 ammo / gun / module / armor / capsule 等子类型） | 342 | 96.5% |
| **recipe（显式图标 413 + 推导 249）** | 662 | **81.8%**（显式 74.4% / 推导 **97.5%**） |
| technology | 277 | **99.2%** |
| fluid | 33 | 96.7% |
| asteroid-chunk | 15 | 94.4% |
| item-group | 12 | 98.8% |
| space-location | 8 | 97.7% |
| quality | 6 | 96.5% |
| tile | 3 | 96.9% |
| surface | 1 | 96.5% |
| **space-connection** | 9 | **36.8%**（画布口径未实现） |

合计 2134 张、平均匹配率 **92.4%**，逐像素完全一致 44 张——「完全一致」要求每个通道都相同，
单层图标剩下的几个百分点主要是**预乘/四舍五入**的 ±1~2，所以主指标是容忍度内的匹配率。
分布：`<50%` 16 张、`50~80%` 317、`80~95%` 126、`95~99%` **1339**、`≥99%` 336。

- 单层图标（占绝大多数）已经很好：`entity`/`item`/`technology` 都在 96%~99%。
- **推导出来的配方图标（97.5%）比显式定义的配方（74.4%）还准**——推导那批基本是单产物、
  单层的简单图标，而显式定义的里面混着大量多层缩放图标（见下），这也反过来印证了推导
  规则本身是对的（249 张 vanilla + 3280 张 py，两个上下文一致）。
- **`recipe` 显式那部分偏低**：最差的一批全是 `*-recycling`（`recycling.png` + 缩小的物品
  图标 + `recycling-top.png` 三层叠加）与画布 66×68 的那批。
- **`space-connection` 偏低**：画布 66×66 的并集包围盒规则（见事实 3）还没实现。
- **带显式 `scale` 的叠加层**还没做到逐像素一致：我们的面积平均重采样与 Factorio 的
  采样核不同，缩放层边缘会有几个像素的差别（视觉上一致，数值上有差）。

### mod 上下文（py，`c3544821b3232cf9`，17381 条原型 / 16888 张参考图）

| 参考图目录 | 张数 | 平均像素匹配率 |
| --- | --- | --- |
| entity | 1603 | 95.6% |
| item | 3300 | 91.4% |
| technology | 957 | 97.8% |
| fluid | 472 | 96.0% |
| item-group | 19 | 97.7% |
| quality | 2 | 98.3% |
| **recipe（显式 6855 + 推导 3280）** | 10135 | **82.7%**（显式 76.5% / 推导 **95.8%**） |
| 其余（space-location / tile / asteroid-chunk） | 5 | 94%~99% |

合计 16493 张、平均匹配率 **87.0%**。分布：`<50%` 97 张、`50~80%` **3926**、`80~95%` 4761、
`95~99%` **7209**、`≥99%` 500——那 3926 张的多数是画布 66×68 之类（并集包围盒规则）
与 `floating` 层的组合，等画布规则实现后应该会整体上移。

### 看图验收（`--sheet`）

把「最差 12 张」拼成「左=我们 / 右=官方」的对照图看：

- vanilla 的 recipe 与总榜：12 行**看不出差别**（缩放层的大小、位置、配色都对上了）；
- py 的总榜：前 3 行是 pyvoid 的 1×1 占位图（两边都是一个点），后面几行是画布 66×68 的
  那批——官方画的是带紫色光环的小图标，我们画的是 64×64 画布下的错位版本，属于上面那条
  待实现的规则。

**这就是「数字过关 + 看着一致」两道关**：`scale` 口径那条就是先看图、再用 `--sweep-multiplier`
量出来的。

## 下一步

1. **实现画布的并集包围盒规则**（事实 3 最后一条）：画布 = 所有层绘制矩形（含 `shift`）的
   并集、取偶数。这一条能同时修掉 `space-connection`（66×66）与 py 那批 66×68 的配方
   （合计 py 有 3926 张落在 50~80% 档，多数是这类画布错位）。
2. 补上**地块**的推导：官方文档写明「没给 `icon`/`icons` 就用 `variants.material_background`」，
   vanilla 150 张、py 62 张（现在全是「推导也画不出」）。需要 codegen 先放行
   `TilePrototype::variants.material_background`（`count`/`line_length`/`picture`/`scale`/`x`/`y`）
   与 `map_color`：实测色块地块（`black-/blue-/red-refined-concrete`）的官方图标 ≈ 素材
   ×`map_color`（`blue` 的 `map_color = {r:0.155, g:0.54, b:0.898, a:0.5}`，官方图标均值
   `(34,66,93)` 对素材均值 `(95,93,88)`），但**确切的取色/帧/缩放口径还没定下来**——
   在没定下来之前不画，免得画出「颜色不对」的图标冒充正确。
   （另外：应用的目录索引里根本没有 `tile` 这一类，所以这块不影响界面。）
3. 修**数据层的 Sprite 图标**：`airborne-pollutant` / `burner-usage` 的 `icon` 是 `Sprite`
   而不是文件名，现在被当成「没有图标定义」（vanilla 3 张、py 1 张；同样不在目录索引里）。
4. 查 py 那 97 张 `<50%` 的：`*-pyvoid` 已经查清（底层 `icon_size = 1`、官方就是 1×1，
   我们的居中取整与官方差一个像素，属于退化情形）。
5. 用更好的重采样（或按 mipmap 级别选择）收敛缩放层的偏差（vanilla 上两种核打平，py 待测）。
6. ~~把「游戏根目录 / mod 目录」写进上下文元数据~~ **改成了记录「启用的 mod + 版本号」**
   （路径只在导出那一刻有意义，图标当时就渲染完了；把路径存进上下文，换机器/换 mod 目录后
   只会误导）。`context.json` 现在写 `game_version` 与 `mods: [{name, version}]`，
   版本号取**游戏实际会加载的那份文件**的版本；`mod-list.json` 读不到、或内嵌 dump /
   用户自备 dump 的情况下留空**不猜**。旧缓存没有这两个字段也能载入（`#[serde(default)]`），
   UI 显示「版本未知 / 无 mod」。
7. **mod 目录的挑法**（Factorio 的约定，用户给的规则 + 在本机实测）：
   - **目录形态**：目录名必须**正好是 mod 的 id**（**带版本号的目录不接受**），`info.json`
     直接躺在目录下（不像 zip 会嵌一层 `<名字>_<版本>/`）；版本取 `info.json.version`，
     没有 `info.json` 的不算 mod。本机的 3 个目录 mod 都是这个形态。
   - **zip 形态**：**文件名必须带版本号**（`<名字>_<版本>.zip`），包内是 `<名字>_<版本>/…`；
     文件名没有版本号的（`.modpack.zip`、`some_mod.zip`）一概不收。
   - **同名多个候选**：`mod-list.json` 里锁定了版本就用那个版本（同版本时**目录形态优先**）；
     没锁定就用**能查到的最新版本**（按版本号逐段比，不是字符串比），无论目录还是 zip。
   - 实测本机：104 条 mod-list（启用 5、锁定版本 0）、**99 个会被加载**；`tanvec-ai-cn`
     装了 5 个版本 → 取 `2026.09.12`；`ForGavin` 4 个 → `2.1.7`；`tanvec-tweaks` 3 个 zip +
     1 个目录 → 目录 `2.1.3`（最新）。**这条以前是看 `read_dir` 顺序的**（谁生效看运气），
     现在确定。想自己核对用 `cargo run -p metatorio-icons --example compare -- --mods-report <mod 目录>`。
