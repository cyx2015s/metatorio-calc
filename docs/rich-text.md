# Factorio 富文本显示与检索

## 调查与数据契约

游戏的 `--dump-prototype-locale` 已将 LocalisedString 求值为字符串，但字符串仍可能含 Factorio 富文本。原来的链路将这些字符串原样写入 `IndexEntry.localized_name`、`PrototypeDetail.localized_name` 和目录 `names`，前端再当普通字符串插值，因此逐字着色会暴露标签并打断搜索词。前端选择器此前只做小写子串匹配，后端 MCP 已有分隔符归一化和错拼匹配，两者口径也不一致。

保留游戏原始字符串、原型 ID、存档和现有 IPC 字段。三个视图各司其职：

- `runtime.richName()`：原始富文本，交给 `RichText.svelte`，用于目标、外部输入、结果列表、选择器搜索结果、悬停标题/配方流、设置中的原型名称等。
- `runtime.localizedName()`：可读纯文本，用于原生 title、aria-label、按钮说明以及没有富文本排版的普通控件。它不再泄漏格式标签。
- 搜索投影：先移除格式元数据，保留可见文字及引用语义，再归一化；不修改查询返回的 ID 或原始本地化字段。

目前 locale 导入器读取的是名称表；本次没有扩展为完整 descriptions/locale-key 数据库。将来若增加说明文本，可复用同一解析器，但需要另行传递说明数据。

## 显示策略

前端只产生受控 token，使用 Svelte 文本节点及组件绘制；不使用 `{@html}`、DOM HTML 注入或任意样式字符串。用户/模组中的 `<img ... onerror=...>` 等仍然是文本。

| 标签 | 处理 |
|---|---|
| `color`、`/color`、`.color` | 支持嵌套恢复；RGB 0–1 / 0–255、命名色、RRGGBB、AARRGGBB。后者转换成 CSS 的 RRGGBBAA |
| `font`、`/font`、`.font` | 保留内容并恢复嵌套状态；bold/mono 映射为应用字体样式，其余继承应用字体，不加载游戏字体或任意资源 |
| 原型引用，例如 item/entity/recipe/fluid/quality/planet | 复用现有 Icon 与上下文图标缓存；缺图标时显示本地化名称或内部 ID；提供纯文本提示和无障碍名称 |
| 引用的 `quality=` | 仅在游戏支持的 item/entity/recipe/fluid/virtual-signal 上显示品质角标；其他引用忽略该参数 |
| `img=class/name`、`img=class.name` | 已知类别走图标缓存；其他 sprite 名称降级为可读名称，不猜测本地文件路径或加载外部 URL |
| tooltip | 保留其可见文字；没有完整 locale-key 数据，暂不模拟该标签的游戏内悬停说明 |
| GPS、列车、站点、平台、盔甲、tip | 显示可读引用；原型上下文没有存档实体和游戏运行状态，不提供传送、打开游戏 GUI 等动作 |
| special-item、space-age | 降级为 Blueprint / Space Age；不展开大段蓝图编码 |
| 未知标签、普通方括号 | 保留字面内容，避免误删模组文字；无效颜色丢弃样式，保留内容 |

这不是游戏渲染器的像素复刻。应用保留自身行高/字号；命名颜色和字体使用应用侧映射。短时等待图标和确实无图标时均可读。解析 token 缓存只保存原始文本与结构、有大小上限，不缓存跨上下文的已解析名称或图标 URL。

## 搜索规则

前端选择器以及 MCP `list_prototypes` / `localized_names` 的比较路径均考虑富文本：

1. 去掉颜色/字体包装，直接拼接文字。因此 `[color=red]量[/color][color=blue]子[/color]点` 可命中“量子点”，不会命中颜色代码。
2. 保留原型内部 ID 检索；内嵌原型引用同时考虑本地化名称与引用 ID。引用名解析有界，不递归展开循环引用。
3. 统一小写，折叠全角 ASCII，忽略空白、下划线、连字符/破折号和常见零宽格式字符。不是删除所有标点，也不是任意 Unicode 音译。
4. 后端精确/前缀/子串/错拼的既有分层仍保留；错拼距离基于纯文本，不对标签源码求编辑距离。排序长度去掉格式包装，避免逐字着色受到额外惩罚。
5. 仅有格式元数据的非空查询不会变成“匹配全部”。没有输入的目录列表仍按既有分组显示。

前端以目录条目预建检索键，不在每个按键事件中重复解析整个目录。查询、可达性和地格能力过滤仍保留，所有选择与 dispatch 使用原型 ID。

## 验证

- `tests/fixtures/rich-text.json`：Rust 与 TypeScript 共享纯文本/归一化样例，覆盖逐字中文、嵌套、品质、图标、异常标签及 HTML 字面量。
- `pnpm test:rich-text`：颜色解析、嵌套恢复、上下文隔离、长蓝图与异常括号。
- `pnpm test:rich-text-browser`：真实 Svelte DOM + mocked IPC，验证逐字着色、悬停、CJK/全角 ID 搜索、标签元数据排除、引用降级和 HTML 不执行。
- `pnpm test:layout`：既有 1000 行机制工作区回归，检查高度、虚拟挂载与交互。
- Rust `rich_text_search_uses_visible_words_and_keeps_raw_labels`：精确/子串/错拼、引用名、原始标签与 ID 保留。

浏览器脚本沿用 `PLAYWRIGHT_MODULE` / `BROWSER_EXECUTABLE`；`UI_TEST_URL` 可显式使用已有测试服务器，未设置时自行启动和关闭 Vite。`RICH_TEXT_SCREENSHOT` 可指定截图路径。

2026-10-19 本次验证：前端纯函数/图标测试 32 项通过；shell 常规测试 49 项通过、1 项忽略，过滤 94 项自动规划资产测试；富文本浏览器回归与 1000 行布局回归通过；前端检查、生产构建与工作区编译通过。前端保留 4 条既有 Selector 初始值捕获警告。没有完成完整 LP 压力矩阵，也没有将 mocked IPC 的浏览器截图当作真实游戏数据验证。

依据：[Factorio Rich text](https://wiki.factorio.com/Rich_text)。游戏存档交互标签的降级属于本应用策略，不能据此推断原游戏行为。
