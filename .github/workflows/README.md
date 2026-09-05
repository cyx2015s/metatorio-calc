# GitHub Workflows

## build-and-release.yml

用 **Tauri** 编译并发布桌面应用（不再是旧的 egui 版），支持 **Linux（x86_64 → AppImage）** 和 **Windows（x86_64 → NSIS setup.exe）**。
macOS 因签名需付费证书暂不构建。

### 触发器

- **push tag `v*.*.*`**：构建并创建/更新 GitHub Release（含 updater 产物）。
- **Release created**：同上。
- **手动触发（workflow_dispatch）**：按 `tauri.conf.json` 里的 `version` 打 tag 建 Release（`v__VERSION__`）。

### 更新机制

采用 `tauri-plugin-updater`：

- **Windows**：下载 `setup.exe` + `.sig`，静默运行安装器完成替换（`installMode: passive`）。
- **Linux**：下载 AppImage + `.sig`，镜像自替换。
- 更新清单由 `tauri-action` 的 `includeUpdaterJson` 生成，端点为
  `https://github.com/cyx2015s/metatorio-calc/releases/latest/download/latest.json`。

### 发布前需要配置的 Secrets / 密钥

因为用了 updater 签名，发布前**必须**：

1. 生成签名密钥对（在本地）：
   ```bash
   cd metatorio-app
   pnpm tauri signer generate --write-keys ~/.tauri/metatorio.key
   ```
2. 在仓库 **Settings → Secrets → Actions**（或某个 **environment** 的 secrets）新增：
   - `TAURI_SIGNING_PRIVATE_KEY`：私钥内容
   - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`：私钥密码
3. 把**公钥**写入 `metatorio-app/src-tauri/tauri.conf.json` 的
   `plugins.updater.pubkey`（二维码下方输出的 `publicKey`），替换当前
   `REPLACE_WITH_TAURI_SIGNER_PUBLIC_KEY` 占位符。

#### 用 environment 存密钥

workflow 的 `publish-tauri` job 通过 `environment` 决定用哪个 environment 的密钥：

```yaml
environment: ${{ vars.RELEASE_ENVIRONMENT || 'release' }}
```

- 若你想用刚创建的 environment，新建一个**仓库变量** `RELEASE_ENVIRONMENT`，
  值设成该 environment 的名字（如 `prod`）。job 就会跑在该 environment，
  `${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}` 会优先取 environment 里的对应密钥。
- 若不设该变量，默认用名为 `release` 的 environment（未创建则回落到仓库级 secrets）。

> 注意：environment 级别的 secret **需要** job 引用该 environment
> （即上面的 `environment:`）才会被注入 `secrets` 上下文。

### 产物

- Windows：`src-tauri/target/release/bundle/nsis/*setup.exe`（含 `.sig` 更新签名）。
- Linux：`src-tauri/target/release/bundle/appimage/*.AppImage`（含 `.sig`）。
- 均由 `tauri-action` 上传到 GitHub Release。
