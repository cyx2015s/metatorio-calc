# 更新机制（两个二进制，一份清单）

发布物有两个：

| 产物 | 包 | 清单里的 target 键 |
|---|---|---|
| GUI 桌面应用（Tauri） | `metatorio-app` | `windows-x86_64-nsis` / `windows-x86_64`、`linux-x86_64-appimage` / `linux-x86_64` |
| 纯 MCP 服务（无 GUI） | `metatorio-headless` | `headless-{os}-{arch}`，例如 `headless-windows-x86_64` |

两者**共用**同一份更新清单、同一把 minisign 密钥、同一个 release；只是清单里的
键不同。**GUI 的配置与行为完全不变。**

## 清单格式

tauri-plugin-updater 的静态清单（`latest.json`），`platforms` 里一个 target 一条：

    {
      "version": "1.5.25",
      "notes": "…",
      "pub_date": "2026-10-09T00:00:00Z",
      "platforms": {
        "windows-x86_64-nsis":     { "signature": "…", "url": "…\Metatorio_1.5.25_x64-setup.exe" },
        "windows-x86_64":          { "signature": "…", "url": "…\Metatorio_1.5.25_x64-setup.exe" },
        "linux-x86_64-appimage":   { "signature": "…", "url": "…\Metatorio_1.5.25_amd64.AppImage" },
        "linux-x86_64":            { "signature": "…", "url": "…\Metatorio_1.5.25_amd64.AppImage" },
        "headless-windows-x86_64": { "signature": "…", "url": "…\metatorio-headless.exe" },
        "headless-linux-x86_64":   { "signature": "…", "url": "…\metatorio-headless" }
      }
    }

`signature` 是 `minisign` 私钥对**该产物文件**的签名（`<file>.sig` 的内容）；
`url` 指向同一个 release 里的资产。

## GUI 端（不动的部分）

`metatorio-app/src-tauri/tauri.conf.json` 的 `plugins.updater` 保持原样：
`endpoints` 指向 `…/releases/latest/download/latest.json`，`pubkey` 不变。

tauri-plugin-updater 在不指定 target 时会按
`{os}-{arch}-{bundle_type}` → `{os}-{arch}` 的顺序查找，所以 GUI 命中的仍是
`windows-x86_64-nsis` / `linux-x86_64-appimage` 这些既有键——**新增 headless
条目不会影响 GUI 的更新判定**。

## headless 端

headless **刻意不链接 tauri**（Linux 上因此不需要 GTK/xvfb），所以不能用
`tauri-plugin-updater`。它在 `metatorio-shell::update` 里实现了一份等价逻辑，
行为对齐：

- target：`headless-{os}-{arch}`（macOS 记作 `darwin`，与 tauri 一致）；
- 版本比较：semver，仅当远端版本 **大于** 当前版本才更新；
- 验签：与 tauri 相同——公钥/签名都是 base64 包了一层的 minisign 文本，
  用 `minisign-verify` 校验，**验签通过前不写任何文件**。

命令行：

    metatorio-headless --check-update     # 只查，打印结果
    metatorio-headless --self-update      # 查 + 下载 + 验签 + 就地替换自己

可选参数 / 环境变量：

| 参数 | 环境变量 | 默认 |
|---|---|---|
| `--update-endpoint` | `METATORIO_UPDATE_ENDPOINT` | 与 GUI 相同的 `latest.json` |
| `--update-pubkey` | `METATORIO_UPDATE_PUBKEY` | 与 GUI 相同的公钥 |
| `--update-target` | `METATORIO_UPDATE_TARGET` | `headless-{os}-{arch}` |

`--self-update` 只**替换**文件，不会自己重启：进程里已经有 MCP 连接与内存态，
静默重启会让调用方莫名断线。交给守护进程（systemd `Restart=`、Windows 服务、
或你自己的 supervisor）在退出后重启即可。替换时旧文件保留为
`<exe>.old`，可手动回滚。

## CI 怎么产出这些条目

`.github/workflows/build-and-release.yml`：

1. `publish-tauri` 矩阵（linux/windows）跑 `tauri-action` 产出 GUI 包与
   `latest.json`（`includeUpdaterJson: true`）——保持不变；
2. 同一 job 里紧接着 `cargo build --release -p metatorio-headless`，用**同一把**
   `TAURI_SIGNING_PRIVATE_KEY` 执行 `pnpm tauri signer sign` 生成
   `metatorio-headless(.exe).sig`，两个文件一起 `gh release upload`；
3. `merge-updater-manifest` job 等两个平台都传完后，下载 release 里的
   `latest.json`，把 headless 的两个 target 条目合并进去再传回。

放在单独 job 是为了避免两个矩阵并行写同一份清单互相覆盖。

## 加一个新的前端 / 新产物

1. 给它选一个不会与别人冲突的 target 键（例如 `gui-egui-windows-x86_64`）；
2. CI 里构建 + 用同一把密钥签名 + 上传资产；
3. 合并进 `latest.json` 的 `platforms`；
4. 客户端用对应 target 去查（tauri 端用 `UpdaterBuilder::target(...)`，非 tauri
   端用 `metatorio_shell::update::check(endpoint, Some(target), version)`）。

清单里各条目彼此独立，加产物不会影响既有客户端的更新判定。
