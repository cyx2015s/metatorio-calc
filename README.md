# 警告

使用自动规划工具前，请自行检查游戏数据不会使得状态空间过于膨胀！！！针对离谱数值机器的枚举微调正在规划中，但不是主要内容。如使用[exfret's randomizer](https://mods.factorio.com/mod/propertyrandomizer)产生插件槽过多的机器。目前的临时限制为：超过8个插件槽位的机器不会尝试枚举空插件，且插件槽过多的机器可能会忽视绝大部分槽位。

# 切向量化工具 Metatorio

切向量化是切向量正在锐意制作中的量化工具，主要用于异星工厂中的问题计算。切向量化使用了顶针的修辞手法，令人忍俊不禁。

# 使用方法

1. （使用新的模组集时）点击右上角状态栏的「数据」，再点「加载游戏数据…」
   1. 选择异星工厂可执行文件的路径
   2. （可选）选择模组文件夹的路径
   3. 确认加载
2. 点击左上角「项目」下拉菜单，选择「新建项目…」；也可以在同一菜单里导入、保存或关闭项目
3. 点击顶部工厂栏右侧的「+」新建工厂
   1. 左栏顶部有两个 tab：
      - 「工厂」：从上到下编辑环境与约束（星球、地表、主品质、严格供给/严格消耗）、生产目标、外部输入、目标表达式
      - 「项目」：编辑时间刻度、品质上限、可达性、里程碑、配方/采矿/无限科技产能，以及自动规划的机器偏好、枚举插件与插件塔
   2. 目标行左侧图标可改流，右键图标可查看能供给它的机制；目标行末尾的「×」在悬停时出现
   3. 在中栏顶部点「+ 添加机制」手动添加游戏机制，包括但不限于配方、采矿、燃烧物品、变质、种植
   4. 点「自动规划」，根据工厂/项目 tab 里的条件尝试自动添加机制。如果失败，可能是问题本身过于复杂，可以尝试添加外部输入、减少产量。建议启用严格供给模式再运行自动规划
   5. 右栏「求解结果」显示总成本与总流平衡；点结果行末尾的「✦」可查看该流的供给/消耗建议
4. 顶部「数据」里可以切换、重命名或删除游戏上下文缓存
5. 应用菜单「⋯」→「字体设置…」可分别指定界面字体与代码字体；设置只保存在本机浏览器，不写入项目文件

工程默认保存为 `.metatorio`（内容仍为 JSON，兼容旧 `.json`／`.fpp`）。可把工程拖到软件图标或窗口中打开；已有 GUI 时会转交并切换到现有窗口。新版安装包提供 `.metatorio` 的双击关联，便携 EXE 不自动注册关联。详见[工程文件与加载链路](docs/project-files.md)。
   
## 产物表达式

（以下为旧版本截图，新版本操作略有不同但结果一致）

现在可以在生产目标中添加表达式。常见需求：计算最优的科研容量生产方式。

![alt text](example/weighted-quality-electromagnetic-science.png)

目前的结论为，原版+SA下只有粉瓶值得生产精良以上品质的：不到一带废料可以生产一带的科研容量。涉及到物流瓶颈另算。

更复杂的表达式可以模拟规划器无法理解的机制。如在后期的太空探索游戏中，制造铁锭运输再解压缩相比制造铁板成铁板运输可以节省约50%的成本。
![alt text](example/iron-ingot.png)
![alt text](example/iron-plate.png)

# 关于模组支持

我已经测试过了[太空探索](https://mods.factorio.com/mod/space-exploration)、 [K2](https://mods.factorio.com/mod/Krastorio2)、[Py](https://mods.factorio.com/mod/pymodpack)和[无主之地](https://mods.factorio.com/mod/nullius)系列，2.0版本和2.1版本的异星工厂游戏数据应该会正常加载，大部分功能都可正常使用。自动规划功能在原版+品质、DLC、太空探索+品质、Py下尝试求解（传说）物品的生产流程时可能失败，在状态空间较多时可能会内存超出，谨慎考虑添加枚举插件、枚举插件塔的数量！

# 构建与开发环境

本仓库横跨 **Rust / C++ / TypeScript** 三种语言，先把工具链一次讲清楚。

## 工具链一览

| 组件 | 版本 | 用途 | 什么时候需要 |
| --- | --- | --- | --- |
| Rust | **≥ 1.85**（edition 2024；CI 用 stable） | 求解器内核（`crates/metatorio-solver` 等）+ Tauri 后端 | 编译一切 |
| C++ 编译器 | 支持 C++11 即可（MSVC 2022 / GCC / Clang） | 编译默认构建中的 HiGHS 后端 | 默认功能或启用 `highs` 时 |
| CMake | **≥ 3.15**（实测 4.1 可用） | HiGHS 的构建系统 | 同上 |
| Ninja（可选） | 任意 | 加速 HiGHS 的 CMake 构建 | 同上 |
| LLVM / libclang | LLVM 14+ | `highs-sys` 用 bindgen 生成 C 绑定 | 同上 |
| Node.js | LTS | 前端构建脚本 | 前端 / 打包 |
| pnpm | 9.x | 前端包管理 | 前端 / 打包 |
| Tauri v2 系统库 | 见下 | 桌面壳 | Linux 打包 |

> 默认构建编译四个项目级后端：Clarabel、HiGHS、microlp 和旧的
> `Rust based`（序列化 id 仍为 `rust-certified`）。新项目默认使用 HiGHS；
> 旧文档没有 `solver` 字段时沿用 Rust based。HiGHS 会通过 CMake 编译内置源码，再用 bindgen 生成绑定，
> 因此需要上表的 C++ / CMake / LLVM。纯 Rust 构建可传
> `--no-default-features --features clarabel,microlp,rust-lp`，或只启用一个后端。
> 后端在项目设置中选择，不再读取 `METATORIO_LP_BACKEND`。

## 按平台装齐

**Windows**（VS 2022 生成工具即可）：

```powershell
winget install Kitware.CMake Ninja-build.Ninja LLVM.LLVM
# 或：choco install cmake ninja llvm -y
# 再把 C:\Program Files\LLVM\bin 加进 PATH（或设 LIBCLANG_PATH 指向它）
```

**Ubuntu / Debian**：

```sh
sudo apt install build-essential cmake ninja-build clang libclang-dev
# Tauri 还需要：
sudo apt install libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf libssl-dev
```

## 常用命令

```sh
# 只跑求解器（最快，不碰前端）
cargo test -p metatorio-solver

# 全量 Rust 测试
cargo test --workspace

# 需要真实游戏数据的长用例（可选，约几分钟）
cargo test --release -p metatorio-runtime --lib legendary_biter_egg -- --ignored --nocapture

# 打包桌面应用
cd metatorio-app && pnpm install && pnpm tauri build
```

# 贡献

自动规划的真实上下文回归、极端倍率、插件塔组合及全产物遍历测试，见 [测试说明与实测结果](docs/auto-plan-testing.md)。

机制卡布局、长列表挂载策略与浏览器回归，见 [机制工作区](docs/mechanic-workspace.md)。
悬停计算详情、消息分层和地格选择边界，见 [悬停详情](docs/hover-details.md)。

欢迎提交各种 issue、pull request 或在其他社交网站上联系我反馈问题和提供建议。

# 许可

见 LICENSE。
