# 图标渲染：为什么自己做、怎么做、怎么验证

## 背景

上下文（游戏数据）的导出流程原本是：调用 `factorio.exe --dump-data`（原型数据）、
`--dump-prototype-locale`（翻译）、`--dump-icon-sprites`（图标贴图），再把 `script-output`
搬进缓存目录。前两步无头可用；**图标这一步在 Steam 版会弹「是否启动游戏」的确认**，
自动流程会被这一下打断。

图标本身并没有那么神秘：原型数据里每个有图标的原型都带 `icon` / `icons`
（`IconData` 层数组），要做的只是「读 PNG → 按规则叠加」。因此改为**在
metatorio 自己的进程内渲染**：导出只剩 `--dump-data` 与 `--dump-prototype-locale`
两条无头命令，`--dump-icon-sprites` 已经不再调用。

实现落在新 crate **`crates/metatorio-icons`**：

| 模块 | 职责 |
| --- | --- |
| `sources` | 把 `__base__/…`、`__core__/…`、`__mod__/…` 解析到真实文件：`<游戏>/data/*` 各目录（含 DLC 的 `space-age`）、mod 目录（解压目录或 `<名字>_<版本>.zip`，按需只读一个条目） |
| `image` | RGBA8 缓冲、PNG 解码/编码（调色板/灰度/16 位归一化）、直通↔预乘、面积平均重采样、source-over 合成 |
| `render` | 按 `IconData` 规则把一层或多层叠成一张图标（[`render_icon`] / [`render_prototype_icon`]）；`render_all_icons` 遍历整个原型仓库，把结果写进缓存目录 |
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
- 完成情况打到 stderr：`图标渲染完成：写出 N 张（<type> N、…），无图标定义 X、缺文件 Y、解码失败 Z`。
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

## 还差什么（已知偏差）

比对**必须把四类账都摆出来**，跳过的东西不数出来就等于假装没问题：

| 类别 | 张数 | 说明 |
| --- | --- | --- |
| 参与比对 | 1885 | 有图标定义 + 官方导出里有图 |
| 官方有图、dump 里没有图标定义 | 437 | 游戏自己拼的图标（配方 249、地块 150、其余 38），**未实现** |
| dump 有定义、官方没导出图 | 0 | 本上下文没有 |
| 官方有图、但原型仓库里没有这条原型 | 429 | `decorative` 163、`virtual-signal` 155、`achievement` 85、`shortcut` 20、其余 6——这些类型不在关注列表，本来就不显示 |
| 同名图在导出里有多份 | 44 | 只用了「原型 type 优先」的那份 |

全量比对（上下文 `2d1e8c21a400155c`，游戏内名为 vanilla-2.1，实为 2.1 + Space Age：
2750 条原型、2744 张官方参考图；`--tolerance 2`）：

| 参考图目录 | 张数 | 平均像素匹配率 |
| --- | --- | --- |
| entity（爆炸 225、尸体 177、树 32、机械 …） | 764 | 97.4% |
| item（含 ammo / gun / module / armor / capsule 等子类型） | 327 | 96.4% |
| technology | 277 | 97.4% |
| fluid | 33 | 96.7% |
| asteroid-chunk | 25 | 93.6% |
| item-group | 12 | 98.8% |
| space-location | 8 | 97.7% |
| quality | 6 | 92.8% |
| ammo-category | 7 | **65.4%** |
| tile | 3 | 96.9% |
| surface | 1 | 96.5% |
| **recipe** | 413 | **69.1%** |
| **space-connection** | 9 | **33.1%** |

合计 1885 张、平均匹配率 90.5%，逐像素完全一致 40 张（2.1%）——「完全一致」要求每个通道
都相同，单层图标剩下的几个百分点主要是**预乘/四舍五入**的 ±1~2，所以主指标是容忍度内的
匹配率。

- 单层图标（占绝大多数）已经很好：`entity`/`item`/`technology` 都在 96%~97%。
- **`recipe` 偏低**：最差的一批全是 `*-recycling`（`recycling.png` + 缩小的物品图标
  `scale = 0.4` + `recycling-top.png` 三层叠加），属于下面「缩放层重采样」这一类。
  另外**249 个配方在 dump 里根本没有 `icons`/`icon`**（图标是游戏按产物自动生成的）、
  150 个地块同理——这两类目前**一张都没渲染**，要覆盖得自己按产物/地块拼图标（还没做）。
- **`space-connection` 偏低**：官方导出是 **66×66**，不是 64×64，而且「只画第 0 层」
  解释不了官方图（差异铺满整幅）——说明这类图标的画布尺寸/摆放另有一套规则，待查。
- **`ammo-category` 偏低（65.4%）**：这类原型的 `icons` 在 dump 里往往只有一层，而官方图
  是把多个物品图画成一个小格阵——同「自动生成」一类，待查。
- **带显式 `scale` 的叠加层**还没做到逐像素一致：我们的面积平均重采样与 Factorio 的
  mipmap 采样核不同，缩放层边缘会有几个像素的差别（视觉上一致，数值上有差）。
  要更贴近需要试更细的滤波核（或按 mipmap 级别取邻近层）。

### mod 上下文（py，`c3544821b3232cf9`，17381 条原型 / 16888 张参考图）

| 参考图目录 | 张数 | 平均像素匹配率 |
| --- | --- | --- |
| entity | 1630 | 95.2% |
| item | 3255 | 90.4% |
| technology | 957 | 97.6% |
| fluid | 472 | 96.0% |
| item-group | 19 | 97.7% |
| **recipe** | 6851 | **53.7%** |
| 其余（quality / space-location / tile / ammo-category / asteroid-chunk） | 13 | 67%~99% |

合计 13197 张、平均匹配率 **72.6%**（比 vanilla 低不少，主因就是 recipe），另外：
**16 张渲染失败全是「缺文件」**——py 的数据里写着 `__pyalienlifegraphics__/graphics/icons//sap-extractor-mk01.png`
（**两个斜杠**），游戏自己会把 `//` 归一化，我们的路径拼接没有，于是 zip 里查不到条目。
这是本 crate 的**真 bug**，不是数据问题。还有 **3584 张官方有图、dump 里没有图标定义**
（配方 3280、`recipe-category` 77、`tile` 62、`module-category` 47 …），全部未渲染。

## 下一步

1. 修掉路径里的 `//`：解析 `__base__`/`__mod__` 路径时把连续斜杠归一化（游戏就是这么做的），
   py 上下文那 16 张「缺文件」应当归零。
2. 查 py 配方为什么只有 53.7%（`*-pyvoid` 那批是 0.00%）：先 `--show recipe/<name>` 看层清单
   与官方图的差异形态，判断是「画布/摆位不一致」还是「官方画的是自动生成的图标」。
3. 补上「自动生成」的图标：配方（按产物排布）、地块、`ammo-category`、`*-category`——这是
   目前最大的一块真空（vanilla 437 张、py 3584 张官方有图、我们一张都不出）。
4. 查清 `space-connection` 的 66×66 画布规则。
5. 用更好的重采样（或按 mipmap 级别选择）收敛缩放层的偏差。
6. 决定 `scale` 口径：`expected × scale` 在全量上略好，但要在 py（16k 张）上复核后再换默认。
7. 把「游戏根目录 / mod 目录」写进上下文元数据（现在只有 `source` 字符串，重新注册同
   内容的上下文时会靠解析字符串补路径，太脆）；顺带让 `Copy`/`None` 来源也能被如实记录。
