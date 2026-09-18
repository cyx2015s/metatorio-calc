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
  [--show <type>/<name> --pixels 36,2;40,2] [--sweep] [--fit-scale]
```

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
3. **导出尺寸 = 原型的 `icon_size`**（未给时按类型默认）：物品/实体/流体 64，
   科技 256，成就/物品组 128；其余一律 64。
   官方文档里 512 只针对 `SpaceLocationPrototype::starmap_icon`、32 只针对
   `ShortcutPrototype::small_icons`——**普通 `icons` 不能套这两个数**（一开始把
   `space-location` 设成 512，`space-location`/`space-connection` 的匹配率掉到 33%，
   因为拿 512 画布去和官方 64 画布比，只比到了中心那一小块）。
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
8. **`scale` 口径**：候选口径做过整扫（`--sweep`，像素匹配率越高越好）：

   | 口径 | 全部类型 1885 张 | 只挑「有层显式给 scale」463 张 |
   | --- | --- | --- |
   | `expected × scale` | **90.81%** | **71.29%** |
   | `icon_size × scale`（当前采用） | 90.54% | 70.21% |
   | `icon_size`（忽略 scale） | 88.25% | 60.89% |
   | `icon_size × scale × 2` | 8.37% | 9.79% |
   | 文档默认 `scale.unwrap_or((expected/2)/icon_size)` | 38.04% | 30.07% |

   结论：**显式 `scale` 确实生效**（忽略它掉 2~10 个百分点），文档那句「默认
   `(expected/2)/icon_size`」在本导出里对不上（按它算整体小一半）。两个候选口径
   （`expected`＝画布边长、`icon_size`＝层自己的 `icon_size`）只在「层显式声明了与画布
   不同的 `icon_size`」时有分歧，全量口径下 `expected × scale` 略好（90.81% vs 90.54%
   ／71.29% vs 70.21%），**当前代码仍是 `icon_size × scale`**——换口径前要在 mod 上下文
   上复核（见下一步）。另外：用「包围盒」反推口径的办法不可靠（叠加层与底层混在一起，
   测出来自相矛盾），最终是靠**整图逐像素匹配率**定的标。
9. **官方导出的目录名不等于原型 `type`**（这是比对工具最初只有 37% 覆盖率的原因）：
   所有**实体类型**（`assembling-machine`、`tree`、`explosion`、`corpse`、`simple-entity`、
   `resource`、`container`、`inserter` …）写在 `entity/` 下，所有**物品子类型**
   （`ammo`、`gun`、`module`、`armor`、`capsule`、`item-with-entity-data` …）写在 `item/`
   下，`planet` 写在 `space-location/` 下，其余类型才是 `<type>/`。所以比对工具改成
   **按文件名建索引、优先取与原型 `type` 同名的目录**，并把实际用到的映射打印出来
   （规律是实测出来的，不是假设的）。修正后覆盖率 1027 → 1885 张。
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
| 同名图在导出里有多份 | 44 | 64 | 只用了「原型 `type` 优先」的那份 |

按参考图目录（vanilla `--tolerance 2`）：

| 参考图目录 | 张数 | 平均像素匹配率 |
| --- | --- | --- |
| entity（爆炸 225、尸体 177、树 32、机械 …） | 764 | 97.4% |
| item（含 ammo / gun / module / armor / capsule 等子类型） | 327 | 96.4% |
| **recipe（显式图标 413 + 推导 249）** | 662 | **79.8%**（显式 69.1% / 推导 **97.5%**） |
| technology | 277 | 97.4% |
| fluid | 33 | 96.7% |
| asteroid-chunk | 25 | 93.6% |
| item-group | 12 | 98.8% |
| space-location | 8 | 97.7% |
| quality | 6 | 92.8% |
| ammo-category | 7 | **65.4%** |
| tile | 3 | 96.9% |
| surface | 1 | 96.5% |
| **space-connection** | 9 | **33.1%** |

合计 2134 张、平均匹配率 91.4%，逐像素完全一致 44 张（2.1%）——「完全一致」要求每个通道
都相同，单层图标剩下的几个百分点主要是**预乘/四舍五入**的 ±1~2，所以主指标是容忍度内的
匹配率。

- 单层图标（占绝大多数）已经很好：`entity`/`item`/`technology` 都在 96%~97%。
- **推导出来的配方图标（97.5%）比显式定义的配方（69.1%）还准**——推导那批基本是单产物、
  单层的简单图标，而显式定义的里面混着大量多层缩放图标（见下），这也反过来印证了推导
  规则本身是对的（249/3280 张、两个上下文一致）。
- **`recipe` 显式那部分偏低**：最差的一批全是 `*-recycling`（`recycling.png` + 缩小的物品
  图标 `scale = 0.4` + `recycling-top.png` 三层叠加），属于下面「缩放层重采样」这一类。
- **`space-connection` 偏低**：官方导出是 **66×66**，不是 64×64，而且「只画第 0 层」
  解释不了官方图（差异铺满整幅）——说明这类图标的画布尺寸/摆放另有一套规则，待查。
- **`ammo-category` 偏低（65.4%）**：这类原型的 `icons` 在 dump 里往往只有一层，而官方图
  是把多个物品图画成一个小格阵——同「无推导可用」一类，待查。
- **带显式 `scale` 的叠加层**还没做到逐像素一致：我们的面积平均重采样与 Factorio 的
  mipmap 采样核不同，缩放层边缘会有几个像素的差别（视觉上一致，数值上有差）。
  要更贴近需要试更细的滤波核（或按 mipmap 级别取邻近层）。

### mod 上下文（py，`c3544821b3232cf9`，17381 条原型 / 16888 张参考图）

| 参考图目录 | 张数 | 平均像素匹配率 |
| --- | --- | --- |
| entity | 1638 | 95.2% |
| item | 3259 | 90.4% |
| technology | 957 | 97.6% |
| fluid | 472 | 96.0% |
| item-group | 19 | 97.7% |
| **recipe（显式 6855 + 推导 3280）** | 10135 | **66.9%**（显式 53.7% / 推导 **94.6%**） |
| 其余（quality / space-location / tile / ammo-category / asteroid-chunk） | 11 | 67%~99% |

合计 16493 张、平均匹配率 **77.0%**（比 vanilla 低，主因是 py 自己定义的配方图标有大量
多层缩放）。py 里推导覆盖了 **3280 张配方**，平均 94.6%。

## 下一步

1. 补上**地块**的推导：官方文档写明「没给 `icon`/`icons` 就用 `variants.material_background`」，
   vanilla 150 张、py 62 张（现在全是「推导也画不出」）。需要 codegen 先放行
   `TilePrototype::variants.material_background`（`count`/`line_length`/`picture`/`scale`/`x`/`y`）。
2. 修**数据层的 Sprite 图标**：`airborne-pollutant` / `burner-usage` 的 `icon` 是 `Sprite`
   而不是文件名，现在被当成「没有图标定义」（vanilla 3 张、py 1 张）。
3. 查 py 显式定义的配方为什么只有 53.7%（`*-pyvoid` 那批是 0.00%）：先
   `--show recipe/<name>` 看层清单与官方图的差异形态，判断是「画布/摆位不一致」还是别的。
4. 查清 `space-connection` 的 66×66 画布规则。
5. 用更好的重采样（或按 mipmap 级别选择）收敛缩放层的偏差。
6. 决定 `scale` 口径：`expected × scale` 在全量上略好，但要在 py（16k 张）上复核后再换默认。
7. 把「游戏根目录 / mod 目录」写进上下文元数据（现在只有 `source` 字符串，重新注册同
   内容的上下文时会靠解析字符串补路径，太脆）；顺带让 `Copy`/`None` 来源也能被如实记录。
