<script module lang="ts">
  import type { ContextInfo } from "$lib/runtime/types";

  /** 上下文来源摘要：游戏版本 + 启用的 mod 数（完整清单在 tooltip）。 */
  export function contextSummary(context: ContextInfo): string {
    const version = context.game_version ?? "版本未知";
    const mods = context.mods.length ? `${context.mods.length} 个 mod` : "无 mod";
    return `${version} · ${mods}`;
  }
</script>

<script lang="ts">
  import { runtime } from "$lib/runtime/store.svelte.ts";
  import { pickGameExecutable, pickModDir } from "$lib/runtime/client";
  import type { ProjectDocument } from "$lib/runtime/types";
  import {
    backdropKeydown,
    type ConfirmState,
  } from "$lib/ui/WorkspaceDialogs.svelte";

  let {
    dataOpen = $bindable(false),
    loadGameOpen = $bindable(false),
    fontSettingsOpen = $bindable(false),
    project,
    onRenameContext,
    onRequestConfirm,
  }: {
    dataOpen: boolean;
    loadGameOpen: boolean;
    fontSettingsOpen: boolean;
    project: ProjectDocument | null;
    onRenameContext: (context: ContextInfo) => void;
    onRequestConfirm: (state: ConfirmState) => void;
  } = $props();

  let gameExePath = $state("");
  let gameModDir = $state("");
  let loadGameError = $state<string | null>(null);
  let pickingExe = $state(false);
  let pickingMod = $state(false);

  let prevLoadGameOpen = false;
  $effect(() => {
    if (loadGameOpen && !prevLoadGameOpen) {
      loadGameError = null;
      gameExePath = "";
      gameModDir = "";
    }
    prevLoadGameOpen = loadGameOpen;
  });

  function openLoadGame() {
    loadGameError = null;
    gameExePath = "";
    gameModDir = "";
    loadGameOpen = true;
  }

  async function browseGameExe() {
    pickingExe = true;
    try {
      const picked = await pickGameExecutable();
      if (picked) gameExePath = picked;
    } finally {
      pickingExe = false;
    }
  }

  async function browseGameMod() {
    pickingMod = true;
    try {
      const picked = await pickModDir();
      if (picked) gameModDir = picked;
    } finally {
      pickingMod = false;
    }
  }

  async function submitLoadGame() {
    const exe = gameExePath.trim();
    if (!exe) {
      loadGameError = "请选择游戏可执行文件";
      return;
    }
    loadGameError = null;
    try {
      await runtime.loadContextFromExecutable(exe, gameModDir.trim() || null);
      loadGameOpen = false;
    } catch (error) {
      loadGameError = String(error);
    }
  }

  /** 上下文 tooltip：逐条列出启用的 mod 与版本。 */
  function contextTitle(context: ContextInfo): string {
    const lines = [`游戏版本：${context.game_version ?? "未知"}`];
    if (context.mods.length === 0) {
      lines.push("启用 mod：无（或这份缓存没有记录）");
    } else {
      lines.push(`启用 mod（${context.mods.length}）：`);
      for (const mod of context.mods) {
        lines.push(`  ${mod.name}${mod.version ? ` ${mod.version}` : ""}`);
      }
    }
    return lines.join("\n");
  }

  function handleWindowKeydown(event: KeyboardEvent): void {
    if (event.key === "Escape" && fontSettingsOpen) {
      fontSettingsOpen = false;
    }
  }
</script>

<svelte:window onkeydown={handleWindowKeydown} />

<!-- ══ 游戏数据（上下文与缓存管理） ══ -->
{#if dataOpen}
  <div
    class="backdrop"
    role="button"
    tabindex="0"
    onclick={(event) => { if (event.target === event.currentTarget) dataOpen = false; }}
    onkeydown={(event) => backdropKeydown(event, () => (dataOpen = false))}
  >
    <div class="modal data-modal" role="dialog" aria-modal="true" aria-label="游戏数据与上下文">
      <header class="modal-head">
        <div class="modal-heading">
          <span class="eyebrow">游戏数据</span>
          <strong>上下文与缓存</strong>
        </div>
        <button class="icon-btn" title="关闭游戏数据" aria-label="关闭游戏数据" onclick={() => (dataOpen = false)}>×</button>
      </header>

      <div class="data-actions">
        <button class="btn primary" onclick={() => { dataOpen = false; openLoadGame(); }}>加载游戏数据…</button>
        <!-- <button
          class="btn"
          title="载入应用内置的示例数据（若当前构建包含）"
          onclick={() => runtime.loadDemoData().catch(() => {})}
        >载入内置示例</button> -->
      </div>

      {#if project && project.context_id && !runtime.contexts.some((c) => c.id === project.context_id)}
        <div class="warn-strip">当前项目引用的游戏上下文已删除或不存在，请从下方列表选一个并点「应用」重新绑定。</div>
      {/if}

      {#if runtime.contexts.length === 0}
        <div class="empty-state">
          <strong>还没有游戏上下文</strong>
          <span class="muted">加载 Factorio 可执行文件，或导入一份数据缓存后即可选择。</span>
          <button class="btn primary" onclick={() => { dataOpen = false; openLoadGame(); }}>加载游戏数据</button>
        </div>
      {:else}
        <div class="context-list">
          {#each runtime.contexts as context (context.id)}
            <div
              class="ctx-row data-row"
              class:active={project
                ? project?.context_id === context.id
                : runtime.activeContext?.id === context.id}
            >
              <div class="ctx-main">
                <div class="ctx-name">
                  <span class="ctx-name-text">{context.name}</span>
                  {#if project?.context_id === context.id}<span class="chip ok">项目使用中</span>{/if}
                  {#if runtime.activeContext?.id === context.id}<span class="chip">全局激活</span>{/if}
                  {#if !context.loaded}<span class="chip">未载入</span>{/if}
                </div>
                <div class="ctx-meta" title={contextTitle(context)}>{contextSummary(context)}</div>
                {#if context.mods.length > 0}
                  <div class="ctx-mods" title={contextTitle(context)}>
                    {context.mods.slice(0, 5).map((mod) => mod.name).join(" · ")}{context.mods.length > 5 ? ` 等 ${context.mods.length} 个` : ""}
                  </div>
                {/if}
              </div>
              <div class="ctx-actions">
                <button
                  class="btn"
                  title={project
                    ? (project?.context_id === context.id
                        ? "当前项目已使用此上下文"
                        : "把当前项目改用此上下文")
                    : (runtime.activeContext?.id === context.id
                        ? "当前为新项目默认上下文"
                        : "设为新项目默认上下文")}
                  disabled={project
                    ? project?.context_id === context.id || runtime.contextBusy
                    : runtime.activeContext?.id === context.id || runtime.contextBusy}
                  onclick={() => {
                    const currentProject = project;
                    if (currentProject) {
                      runtime.setProjectContext(currentProject.id, context.id).catch(() => {});
                    } else {
                      runtime.setActiveContext(context.id).catch(() => {});
                    }
                  }}
                >应用</button>
                <button class="icon-btn" title="重命名上下文" aria-label="重命名上下文" onclick={() => onRenameContext(context)}>✎</button>
                <button
                  class="icon-btn danger"
                  title="删除该上下文的缓存"
                  aria-label="删除上下文缓存"
                  onclick={() =>
                    onRequestConfirm({
                      message: `删除上下文「${context.name}」的缓存？`,
                      confirmLabel: "删除",
                      danger: true,
                      action: () => runtime.deleteContext(context.id).catch(() => {}),
                    })}
                >×</button>
              </div>
            </div>
          {/each}
        </div>
        {#if project}
          {@const projCtx = runtime.contexts.find((c) => c.id === project.context_id)}
          {#if projCtx && projCtx.icon_root}
            <div class="kv"><span>图标目录</span><span class="mono small" title={projCtx.icon_root}>{projCtx.icon_root}</span></div>
          {/if}
        {/if}
      {/if}

      <footer class="modal-foot">
        <span class="muted">上下文只决定可用原型与图标；项目文档独立保存。</span>
        <button class="btn ghost" onclick={() => (dataOpen = false)}>关闭</button>
      </footer>
    </div>
  </div>
{/if}

<!-- ══ 字体设置（纯前端，localStorage） ══ -->
{#if fontSettingsOpen}
  <div
    class="backdrop"
    role="button"
    tabindex="0"
    onclick={(event) => { if (event.target === event.currentTarget) fontSettingsOpen = false; }}
    onkeydown={(event) => backdropKeydown(event, () => (fontSettingsOpen = false))}
  >
    <div class="mini-modal font-modal" role="dialog" aria-modal="true" aria-label="字体设置">
      <div class="mini-title">字体设置</div>
      <p class="font-hint">分别指定界面文字与数值/代码使用的字体。设置只保存在本机浏览器中，不写入项目文件。</p>

      <div class="font-section">
        <label for="ui-font">界面字体</label>
        <input
          id="ui-font"
          type="text"
          list="ui-font-presets"
          value={runtime.uiFont}
          placeholder="默认：Inter / 系统 UI"
          spellcheck="false"
          oninput={(event) => runtime.setUiFont(event.currentTarget.value)}
        />
        <datalist id="ui-font-presets">
          <option value="system-ui"></option>
          <option value="Microsoft YaHei"></option>
          <option value="Noto Sans SC"></option>
          <option value="PingFang SC"></option>
          <option value="LXGW WenKai"></option>
          <option value="Sarasa Gothic SC"></option>
          <option value="Inter"></option>
          <option value="IBM Plex Sans"></option>
          <option value="Segoe UI"></option>
        </datalist>
        <div class="font-preview ui">
          <strong>切向量化</strong>
          <span>生产目标 5.0/s · 电磁科技包（粉瓶）· 已启用 · 114.514</span>
        </div>
      </div>

      <div class="font-section">
        <label for="mono-font">代码字体</label>
        <input
          id="mono-font"
          type="text"
          list="mono-font-presets"
          value={runtime.monoFont}
          placeholder="默认：JetBrains Mono / 等宽"
          spellcheck="false"
          oninput={(event) => runtime.setMonoFont(event.currentTarget.value)}
        />
        <datalist id="mono-font-presets">
          <option value="JetBrains Mono"></option>
          <option value="Cascadia Code"></option>
          <option value="Fira Code"></option>
          <option value="Sarasa Term SC Nerd"></option>
          <option value="Consolas"></option>
          <option value="SF Mono"></option>
          <option value="Sarasa Mono SC"></option>
          <option value="ui-monospace"></option>
        </datalist>
        <div class="font-preview mono">
          <span>SC 128.426 · iron-plate 43.8/s · 1,234.56 · 0x1F600</span>
        </div>
      </div>

      <p class="font-hint">输入系统已安装的字体名；留空恢复默认。也可以直接粘贴完整的 font-family 列表（逗号分隔）。</p>
      <div class="mini-actions">
        <button class="btn ghost" onclick={() => runtime.resetFonts()}>恢复默认</button>
        <button class="btn primary" onclick={() => (fontSettingsOpen = false)}>完成</button>
      </div>
    </div>
  </div>
{/if}

{#if loadGameOpen}
  <div class="backdrop" role="button" tabindex="0" onclick={(e) => { if (e.target === e.currentTarget) loadGameOpen = false; }} onkeydown={(e) => backdropKeydown(e, () => (loadGameOpen = false))}>
    <div class="mini-modal load-game">
      <div class="mini-title">加载游戏数据</div>
      <div class="path-field">
        <div class="path-label">游戏可执行文件（必选）</div>
        <div class="path-row">
          <input
            bind:value={gameExePath}
            placeholder="游戏可执行文件路径"
            onkeydown={(event) => {
              if (event.key === "Enter") submitLoadGame();
            }}
          />
          <button class="btn" onclick={browseGameExe} disabled={pickingExe}>
            {pickingExe ? "…" : "浏览"}
          </button>
        </div>
      </div>
      <div class="path-field">
        <div class="path-label">Mod 目录（可选，留空 = 原版）</div>
        <div class="path-row">
          <input
            bind:value={gameModDir}
            placeholder="留空使用原版"
            onkeydown={(event) => {
              if (event.key === "Enter") submitLoadGame();
            }}
          />
          <button class="btn" onclick={browseGameMod} disabled={pickingMod}>
            {pickingMod ? "…" : "浏览"}
          </button>
        </div>
      </div>
      {#if loadGameError}
        <div class="err-text">{loadGameError}</div>
      {/if}
      <div class="mini-actions">
        <button class="btn ghost" onclick={() => (loadGameOpen = false)}>取消</button>
        <button class="btn primary" onclick={submitLoadGame} disabled={runtime.contextBusy}>
          {runtime.contextBusy ? "加载中…" : "加载"}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .warn-strip {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    padding: 8px 10px;
    margin: 0 0 10px;
    color: var(--warn);
    background: var(--warn-dim);
    border: 1px solid var(--warn-line);
    border-radius: var(--radius-sm);
    font-size: 11px;
    line-height: 1.45;
  }

  .empty-state {
    display: grid;
    justify-items: center;
    gap: 6px;
    padding: 56px 24px;
    color: var(--muted);
    border: 1px dashed var(--line-strong);
    border-radius: var(--radius);
    text-align: center;
  }

  .empty-state strong {
    color: var(--text);
    font-size: 12.5px;
  }

  .empty-state .muted {
    max-width: 360px;
    font-size: 11.5px;
    line-height: 1.6;
  }

  .backdrop {
    position: fixed;
    z-index: 50;
    inset: 0;
    display: grid;
    place-items: center;
    padding: 24px;
    background: var(--backdrop);
    backdrop-filter: blur(6px);
  }

  .modal {
    width: min(560px, 100%);
    max-height: min(760px, calc(100vh - 48px));
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 18px;
    overflow: hidden;
    background: var(--panel);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius);
    box-shadow: var(--shadow-strong);
  }

  .modal.data-modal {
    width: min(780px, 100%);
  }

  .modal-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
    padding-bottom: 12px;
    border-bottom: 1px solid var(--line);
  }

  .modal-heading {
    display: grid;
    gap: 3px;
  }

  .modal-heading strong {
    font-size: 14px;
    font-weight: 700;
    letter-spacing: 0.02em;
  }

  .data-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .context-list {
    display: grid;
    gap: 6px;
    min-height: 0;
    overflow-y: auto;
    padding-right: 2px;
  }

  .ctx-row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 12px;
    background: var(--panel-elev);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    transition: background var(--transition-fast), border-color var(--transition-fast);
  }

  .ctx-row:hover {
    background: var(--card-hover);
    border-color: var(--line-strong);
  }

  .ctx-row.active {
    background: var(--accent-dim);
    border-color: var(--accent-line);
    box-shadow: inset 2px 0 0 var(--accent);
  }

  .ctx-main {
    min-width: 0;
    flex: 1;
    display: grid;
    gap: 3px;
  }

  .ctx-name {
    min-width: 0;
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
    overflow: hidden;
    font-size: 12px;
    font-weight: 650;
  }

  .ctx-name-text {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .ctx-meta {
    min-width: 0;
    overflow: hidden;
    color: var(--muted);
    font-family: var(--mono);
    font-size: 9.5px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .ctx-mods {
    min-width: 0;
    overflow: hidden;
    color: var(--faint);
    font-size: 9px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .ctx-actions {
    flex: 0 0 auto;
    display: flex;
    align-items: center;
    gap: 3px;
  }

  .modal-foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding-top: 12px;
    border-top: 1px solid var(--line);
    font-size: 10px;
  }

  .kv {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 5px 0;
    color: var(--muted);
    font-size: 10.5px;
  }

  .kv .small {
    overflow: hidden;
    max-width: 220px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .mini-modal {
    width: min(352px, 100%);
    display: grid;
    gap: 14px;
    padding: 18px;
    background: var(--panel);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius);
    box-shadow: var(--shadow-strong);
  }

  .mini-modal.load-game {
    width: min(480px, 100%);
  }

  .font-modal {
    width: min(520px, 100%);
  }

  .font-section {
    display: grid;
    gap: 5px;
  }

  .font-section > label {
    color: var(--faint);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.1em;
    text-transform: uppercase;
  }

  .font-section input {
    width: 100%;
    font-size: 12px;
  }

  .font-preview {
    min-width: 0;
    display: grid;
    gap: 3px;
    padding: 9px 11px;
    overflow: hidden;
    background: var(--input-bg);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
  }

  .font-preview.ui {
    font-family: var(--sans);
  }

  .font-preview.mono {
    font-family: var(--mono);
    font-size: 12px;
  }

  .font-preview strong {
    overflow: hidden;
    font-size: 14px;
    font-weight: 650;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .font-preview span {
    overflow: hidden;
    color: var(--muted);
    font-size: 11px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .font-hint {
    margin: 0;
    color: var(--muted);
    font-size: 10px;
    line-height: 1.55;
  }

  .mini-title {
    display: flex;
    align-items: center;
    gap: 6px;
    padding-bottom: 10px;
    border-bottom: 1px solid var(--line);
    font-size: 13.5px;
    font-weight: 700;
    letter-spacing: 0.02em;
  }

  .mini-modal input {
    min-height: 32px;
    padding: 0 9px;
    color: var(--text);
    background: var(--input-bg);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-sm);
    font-size: 12px;
    transition: border-color var(--transition-fast), box-shadow var(--transition-fast);
  }

  .mini-modal input:focus {
    border-color: var(--accent);
    outline: none;
    box-shadow: 0 0 0 1px var(--accent);
  }

  .mini-actions {
    display: flex;
    justify-content: flex-end;
    gap: 6px;
    padding-top: 12px;
    margin-top: 2px;
    border-top: 1px solid var(--line);
  }

  .path-field {
    display: grid;
    gap: 5px;
  }

  .path-label {
    color: var(--faint);
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.04em;
  }

  .path-row {
    display: flex;
    gap: 6px;
  }

  .path-row input {
    flex: 1;
    min-width: 0;
  }

  .err-text {
    color: var(--danger);
    font-size: 11px;
    line-height: 1.45;
  }
</style>
