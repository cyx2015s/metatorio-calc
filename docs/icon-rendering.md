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
  [--fit-scale] [--sheet <对照图.png> [--sheet-count 12]] [--render-only <目录>]
```

`--sheet` 是**给人眼看的**：把匹配率最差的若干张拼成「左=我们 / 右=官方、白底=透明」的
对照图——数字过关不等于看着像（`scale` 口径那条就是这么发现的），反过来也一样。
`--render-only` 只跑应用注册上下文时的那一步并计时（不读参考图），用来估时间。

**性能实测**（release / debug 都测过，同一台机器、同一份 dump；`--render-only` 只跑应用
注册上下文那一步，含解码/缩放/编码/写盘）：

| 上下文 | 图标数 | debug | release |
| --- | --- | --- | --- |
| vanilla | 2134 | 20.0s（9.4 ms/张） | **2.3s（1.1 ms/张，910 张/秒）** |
| py | 16493 | ——（按 debug 比例约 40 分钟） | **455.8s（27.6 ms/张，36 张/秒）** |

- release 比 debug 快 **约 8.7 倍**——**调参/验收一律用 `--release`**，否则一个全量比对
  就是几十分钟起步。
- py 每张比 vanilla 贵 25 倍（贴图更大、层更多、还要读 zip），一次上下文注册要
  **7.6 分钟**——这是下一个要处理的点。

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
- 渲染走 `render_record_icon_with`：自带 `icon`/`icons` 的按定义画；没有的按官方写明的
  **推导**画（配方 → 主产物/唯一产物）。
- 完成情况打到 stderr：`图标渲染完成：写出 N 张（其中按官方规则推导 D 张；<type> N、…），
  无图标定义 X、推导失败 W、缺文件 Y、解码失败 Z`——推导出来的和画不出来的都不能省。
- 测试 `context_registration_renders_icons_without_running_the_game` 串起「注册 → 渲染 →
  读回 PNG」整条链（本机没有 `<游戏>/data/base` 时打印 `[skip]` 跳过）。

## 已经查清的事实（都是实测，不是猜的）

1. **游戏导出的图标是「预乘 alpha」**，源 PNG 是**直通 alpha**。
   实测：导出像素 ≈ 源像素 RGB × A/255（例如源 `(159,159,159,44)` → 导出 `(27,27,27,44)`）。
   所以比对时要把我们自己渲染的直通结果**预乘一次**再比（`Rgba8::premultiplied`）；
   直接比原始值会把「预乘」误判成大面积差异——这正是最初 4 个图标「看起来一样、
   像素差一大截」的原因。
2. **图标文件是「mipmap 横排」**：`graphics/icons/iron-plate.png` 是 120×64 =
   64+32+16+8，level 0 就是左上角的 64×64。渲染取这一块即可。
3. **画布尺寸的口径**（这条改过两次，都是被实测逼出来的）：
   - 只有 `icon`（老式单层图标）时，画布 = **原型的 `icon_size`**，没给就按类型默认
     （物品/实体/流体 64，科技 256，成就/物品组 128）；
   - 给了 `icons` 时，**原型的 `icon_size` 根本不参与**（官方文档原文就是 "Only loaded if
     `icons` is not defined"），画布 = **第 0 层的 `icon_size`**（没给则类型默认）。
     实测三例（py 配方）：原型 `icon_size = 32` + 层 `[256, 32]` → 官方 **256×256**；
     层 `[64, 32]` → **64×64**；层 `[1, 32]`（pyvoid 的占位底层）→ 官方就是 **1×1**。
     一开始按原型的 32 去画，整个 py 上下文的配方匹配率被钉在 53.7%。
   - 官方文档里 512 只针对 `SpaceLocationPrototype::starmap_icon`、32 只针对
     `ShortcutPrototype::small_icons`——**普通 `icons` 不能套这两个数**（一开始把
     `space-location` 设成 512，`space-location`/`space-connection` 的匹配率掉到 33%，
     因为拿 512 画布去和官方 64 画布比，只比到了中心那一小块）。
   - 还有一类**画布比 64 大一点**的：`space-connection` 官方是 66×66，py 某些配方是
     66×68。**这类已经找到规律，只是还没实现**：画布不是固定的，而是**所有层绘制矩形的
     并集包围盒**（含 `shift`），取偶数。验算 py 的
     `recipe/Phadai-Dance-Dance-Revolution-2-dubstep`（层：64×0.5、64×0.25 shift(9,9)、
     40×0.35 shift(10,-10)，按「×2」口径 → 64 / 32 / 28 像素）：x 并集 `[-32,34]` = 66、
     y 并集 `[-34,34]` = 68，**与官方 66×68 完全一致**；`space-connection` 按同一算法得
     66~67，也对得上。现在固定用第 0 层的层边长当画布，所以这两类整体错位（36.8% / ~14%）。
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
6. 把「游戏根目录 / mod 目录」写进上下文元数据（现在只有 `source` 字符串，重新注册同
   内容的上下文时会靠解析字符串补路径，太脆）；顺带让 `Copy`/`None` 来源也能被如实记录。
