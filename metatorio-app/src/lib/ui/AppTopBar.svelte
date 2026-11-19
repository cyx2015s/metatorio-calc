<script lang="ts">
  import { onMount } from "svelte";
  import { getVersion } from "@tauri-apps/api/app";
  import { check as updaterCheck } from "@tauri-apps/plugin-updater";
  import { relaunch } from "@tauri-apps/plugin-process";
  import { runtime } from "$lib/runtime/store.svelte.ts";
  import type {
    ContextInfo,
    FactoryDocument,
    ProjectDocument,
  } from "$lib/runtime/types";
  import { contextSummary } from "$lib/ui/GameDataModal.svelte";
  import type {
    CloseProjectState,
    ConfirmState,
  } from "$lib/ui/WorkspaceDialogs.svelte";

  let {
    project,
    factory,
    projectContext,
    onOpenData,
    onOpenLoadGame,
    onOpenFontSettings,
    onNewProject,
    onNewFactory,
    onNewUniverse,
    onRenameProject,
    onRenameFactory,
    onCloseProject,
    onRequestConfirm,
    onShowNotice,
    onMenuOpen,
  }: {
    project: ProjectDocument | null;
    factory: FactoryDocument | null;
    projectContext: ContextInfo | null;
    onOpenData: () => void;
    onOpenLoadGame: () => void;
    onOpenFontSettings: () => void;
    onNewProject: () => void;
    onNewFactory: () => void;
    onNewUniverse: () => void;
    onRenameProject: () => void;
    onRenameFactory: (name: string) => void;
    onCloseProject: (state: CloseProjectState) => void;
    onRequestConfirm: (state: ConfirmState) => void;
    onShowNotice: (message: string) => void;
    onMenuOpen?: () => void;
  } = $props();

  // 应用版本号（来自 tauri.conf.json version，无则空串）
  let appVersion = $state("");
  let projectMenuOpen = $state(false);
  let factoryMenuOpen = $state(false);
  let appMenuOpen = $state(false);
  let updating = $state(false);

  /** 当前主题的可读标签（应用菜单里显示当前选中项）。 */
  let themeLabel = $derived(
    runtime.theme === "system" ? "跟随系统" : runtime.theme === "light" ? "亮色" : "深色",
  );

  onMount(() => {
    getVersion().then((v) => (appVersion = v)).catch(() => {});
  });

  function closeMenus() {
    projectMenuOpen = false;
    factoryMenuOpen = false;
    appMenuOpen = false;
    onMenuOpen?.();
  }

  async function checkForUpdate() {
    if (updating) return;
    updating = true;
    try {
      const update = await updaterCheck();
      if (!update) {
        onShowNotice("当前已是最新版本");
        return;
      }
      onRequestConfirm({
        message: `发现新版本 ${update.version}，是否立即下载并重启更新？`,
        confirmLabel: "立即更新",
        action: () => {
          onShowNotice("正在下载更新…");
          update
            .downloadAndInstall()
            .then(() => relaunch())
            .catch((error) => onShowNotice(`更新失败：${String(error)}`));
        },
      });
    } catch (error) {
      onShowNotice(`检查更新失败：${String(error)}`);
    } finally {
      updating = false;
    }
  }

  function handleWindowKeydown(event: KeyboardEvent): void {
    if (event.key === "Escape" && (projectMenuOpen || factoryMenuOpen || appMenuOpen)) {
      closeMenus();
    }
  }
</script>

<svelte:window onkeydown={handleWindowKeydown} />

<header class="appbar">
  <div class="brand" title="切向量化 · 异星工厂产线求解器">
    <span class="brand-mark">切</span>
    <span class="brand-name">向量化</span>
  </div>

  <!-- 项目：切换 / 新建 / 导入 / 保存 / 关闭都收进一个菜单 -->
  <div class="menu-wrap">
    <button
      class="project-trigger"
      aria-expanded={projectMenuOpen}
      title={project ? `当前项目：${project.name} · 点击切换或管理项目` : "选择或新建项目"}
      onclick={() => {
        projectMenuOpen = !projectMenuOpen;
        factoryMenuOpen = false;
        appMenuOpen = false;
        onMenuOpen?.();
      }}
    >
      <span class="trigger-label">项目</span>
      <span class="trigger-value">{project?.name ?? "未选择项目"}</span>
      <span class="caret">{projectMenuOpen ? "▴" : "▾"}</span>
    </button>
    {#if projectMenuOpen}
      <div class="menu-catcher" aria-hidden="true" onclick={() => (projectMenuOpen = false)}></div>
      <div class="menu project-menu">
        <div class="menu-label">切换项目</div>
        {#each runtime.document?.projects ?? [] as item (item.id)}
          <button
            class:active={item.id === runtime.selectedProject?.id}
            title={item.id === runtime.selectedProject?.id ? "当前项目" : `切换到项目：${item.name}`}
            onclick={() => {
              projectMenuOpen = false;
              runtime.selectProject(item.id).catch(() => {});
            }}
          >
            <span class="menu-item-main">{item.name}</span>
            <span class="menu-meta">{item.factories.length} 工厂</span>
          </button>
        {:else}
          <div class="menu-empty">还没有项目</div>
        {/each}
        <div class="menu-sep"></div>
        <button onclick={() => { projectMenuOpen = false; onNewProject(); }}>新建项目…</button>
        <button
          disabled={runtime.busy}
          onclick={() => { projectMenuOpen = false; runtime.openProject().catch(() => {}); }}
        >导入项目…</button>
        {#if project}
          {@const currentProject = project}
          <div class="menu-sep"></div>
          <button onclick={() => { projectMenuOpen = false; onRenameProject(); }}>重命名项目…</button>
          <button
            disabled={runtime.busy}
            title={runtime.projectSavePath(currentProject.id) ? `保存到 ${runtime.projectSavePath(currentProject.id)}` : "保存项目（首次保存会询问位置）"}
            onclick={() => { projectMenuOpen = false; runtime.saveCurrentProject().catch(() => {}); }}
          >保存项目</button>
          <button
            disabled={runtime.busy}
            onclick={() => { projectMenuOpen = false; runtime.saveProjectAs().catch(() => {}); }}
          >另存为…</button>
          <button
            class="danger"
            onclick={() => {
              projectMenuOpen = false;
              onCloseProject({ project: currentProject.id, name: currentProject.name });
            }}
          >关闭项目…</button>
        {/if}
      </div>
    {/if}
  </div>

  <!-- 工厂切换与当前工厂操作 -->
  {#if project}
    <div class="factory-bar">
      <span class="bar-label">工厂</span>
      <div class="factory-tabs">
        {#each project.factories as item (item.id)}
          <button
            class="ftab"
            class:active={item.id === runtime.selectedFactory?.id}
            title={item.id === runtime.selectedFactory?.id ? `当前工厂：${item.name}` : `切换到工厂：${item.name}`}
            onclick={() => runtime.selectFactory(item.id).catch(() => {})}
          >{item.name}</button>
        {/each}
        {#each project.universes ?? [] as item (item.id)}
          <button
            class="ftab universe"
            class:active={item.id === runtime.selectedUniverseId}
            title={item.id === runtime.selectedUniverseId ? `当前宇宙工厂：${item.name}` : `切换到宇宙工厂：${item.name}`}
            onclick={() => runtime.selectUniverse(item.id).catch(() => {})}
          >{item.name}</button>
        {/each}
        <button class="ftab add" title="新建工厂" onclick={onNewFactory}>+</button>
        <button class="ftab add universe-add" title="新建宇宙工厂" onclick={onNewUniverse}>✦</button>
      </div>
      {#if factory}
        {@const currentFactory = factory}
        <div class="menu-wrap">
          <button
            class="icon-btn subtle"
            title={`当前工厂操作：${currentFactory.name}（重命名 / 克隆 / 删除）`}
            aria-expanded={factoryMenuOpen}
            onclick={() => {
              factoryMenuOpen = !factoryMenuOpen;
              projectMenuOpen = false;
              appMenuOpen = false;
              onMenuOpen?.();
            }}
          >⋯</button>
          {#if factoryMenuOpen}
            <div class="menu-catcher" aria-hidden="true" onclick={() => (factoryMenuOpen = false)}></div>
            <div class="menu factory-menu">
              <div class="menu-label">{currentFactory.name}</div>
              <button onclick={() => { factoryMenuOpen = false; onRenameFactory(currentFactory.name); }}>重命名工厂…</button>
              <button onclick={() => { factoryMenuOpen = false; runtime.cloneFactory(currentFactory.id).catch(() => {}); }}>克隆工厂</button>
              <button class="danger" onclick={() => { factoryMenuOpen = false; runtime.removeFactory(currentFactory.id).catch(() => {}); }}>删除工厂…</button>
            </div>
          {/if}
        </div>
      {/if}
    </div>
  {/if}

  <span class="spacer"></span>

  <!-- 数据状态：点击打开游戏数据/上下文管理 -->
  <button
    class="status-pill"
    class:busy={runtime.contextBusy}
    title={projectContext
      ? `游戏数据：${projectContext.name}（${contextSummary(projectContext)}）\n点击管理数据上下文`
      : "尚未加载游戏数据 · 点击载入或选择上下文"}
    onclick={() => { closeMenus(); onOpenData(); }}
  >
    <span class="status-dot"></span>
    <span class="status-text">
      {runtime.contextBusy ? "正在加载数据…" : projectContext?.name ?? "未加载游戏数据"}
    </span>
    <span class="status-action">数据</span>
  </button>

  <div class="undo-group">
    <button
      class="icon-btn"
      title={runtime.history.undo_label
        ? `撤销：${runtime.history.undo_label}（Ctrl+Z）`
        : "没有可撤销的操作"}
      aria-label="撤销"
      onclick={() => runtime.undo().catch(() => {})}
      disabled={!runtime.history.can_undo || runtime.busy}
    >↶</button>
    <button
      class="icon-btn"
      title={runtime.history.redo_label
        ? `重做：${runtime.history.redo_label}（Ctrl+Shift+Z / Ctrl+Y）`
        : "没有可重做的操作"}
      aria-label="重做"
      onclick={() => runtime.redo().catch(() => {})}
      disabled={!runtime.history.can_redo || runtime.busy}
    >↷</button>
  </div>

  <!-- 应用菜单：外观 / 加载数据 / 更新 / 版本 -->
  <div class="menu-wrap">
    <button
      class="icon-btn"
      aria-label="应用菜单"
      aria-expanded={appMenuOpen}
      title="应用菜单：外观、字体、加载数据、检查更新、版本"
      onclick={() => {
        appMenuOpen = !appMenuOpen;
        projectMenuOpen = false;
        factoryMenuOpen = false;
        onMenuOpen?.();
      }}
    >⋯</button>
    {#if appMenuOpen}
      <div class="menu-catcher" aria-hidden="true" onclick={() => (appMenuOpen = false)}></div>
      <div class="menu app-menu">
        <div class="menu-label">外观</div>
        <div class="theme-switch" role="group" aria-label="主题">
          <button
            class:active={runtime.theme === "system"}
            title="跟随系统深色 / 亮色外观"
            onclick={() => runtime.setTheme("system")}
          >系统</button>
          <button
            class:active={runtime.theme === "light"}
            title="使用亮色外观"
            onclick={() => runtime.setTheme("light")}
          >亮色</button>
          <button
            class:active={runtime.theme === "dark"}
            title="使用深色外观"
            onclick={() => runtime.setTheme("dark")}
          >深色</button>
        </div>
        <button
          title="分别选择界面字体与代码字体，设置只保存在本机"
          onclick={() => { appMenuOpen = false; onOpenFontSettings(); }}
        >字体设置…</button>
        <div class="menu-sep"></div>
        <button onclick={() => { appMenuOpen = false; onOpenLoadGame(); }}>加载游戏数据…</button>
        <button disabled={updating} onclick={() => { appMenuOpen = false; checkForUpdate(); }}>
          {updating ? "正在检查更新…" : "检查更新…"}
        </button>
        <div class="menu-note">
          切向量化 v{appVersion || "—"} · 当前 {themeLabel}
        </div>
      </div>
    {/if}
  </div>
</header>

<style>
  .appbar {
    position: relative;
    z-index: 20;
    flex: 0 0 auto;
    height: 52px;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 14px;
    background: var(--panel);
    border-bottom: 1px solid var(--line);
    box-shadow: var(--shadow-sm);
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: 0 0 auto;
    user-select: none;
  }

  .brand-mark {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    color: var(--accent-ink);
    background: var(--accent);
    border: 1px solid color-mix(in srgb, var(--accent) 80%, var(--line));
    border-radius: var(--radius-sm);
    box-shadow: var(--shadow-sm);
    font-size: 12px;
    font-weight: 800;
    letter-spacing: 0;
    line-height: 1;
  }

  .brand-name {
    color: var(--text);
    font-size: 11px;
    font-weight: 750;
    letter-spacing: 0.2em;
  }

  .spacer {
    flex: 1 1 auto;
  }

  .menu-wrap {
    position: relative;
    display: inline-flex;
    align-items: center;
  }

  .menu-catcher {
    position: fixed;
    z-index: 39;
    inset: 0;
  }

  /* 项目切换按钮：把项目名做成可点的主入口。 */
  .project-trigger {
    height: 34px;
    max-width: 290px;
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 0 10px;
    color: var(--text);
    background: var(--panel-elev);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    box-shadow: var(--shadow-sm);
    cursor: pointer;
    transition:
      border-color var(--transition-fast),
      background var(--transition-fast),
      box-shadow var(--transition-fast);
  }

  .project-trigger:hover,
  .project-trigger[aria-expanded="true"] {
    background: var(--card-hover);
    border-color: var(--line-strong);
  }

  .trigger-label {
    color: var(--faint);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.1em;
  }

  .trigger-value {
    min-width: 0;
    overflow: hidden;
    font-size: 12px;
    font-weight: 650;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .caret {
    flex: 0 0 auto;
    color: var(--faint);
    font-size: 9px;
  }

  /* 工厂段：与左侧项目区分，用竖线隔开。 */
  .factory-bar {
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 7px;
    padding-left: 10px;
    border-left: 1px solid var(--line);
  }

  .bar-label {
    flex: 0 0 auto;
    color: var(--faint);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.12em;
  }

  .factory-tabs {
    min-width: 0;
    max-width: min(42vw, 620px);
    display: flex;
    align-items: center;
    gap: 3px;
    overflow-x: auto;
    overflow-y: hidden;
    scrollbar-width: none;
  }

  .factory-tabs::-webkit-scrollbar {
    display: none;
  }

  .ftab {
    height: 32px;
    max-width: 190px;
    display: inline-flex;
    align-items: center;
    padding: 0 11px;
    overflow: hidden;
    color: var(--muted);
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    font-size: 11.5px;
    font-weight: 550;
    text-overflow: ellipsis;
    white-space: nowrap;
    cursor: pointer;
    transition:
      color var(--transition-fast),
      background var(--transition-fast),
      border-color var(--transition-fast),
      box-shadow var(--transition-fast);
  }

  .ftab:hover {
    color: var(--text);
    background: var(--card-hover);
  }

  .ftab.active {
    color: var(--text);
    background: var(--accent-dim);
    border-color: var(--line-strong);
    box-shadow: inset 0 -2px 0 var(--accent);
    font-weight: 700;
  }

  .ftab.universe {
    border-style: dashed;
    border-color: var(--line);
    background: color-mix(in srgb, var(--panel-elev) 65%, transparent);
  }

  .ftab.universe:hover {
    border-style: solid;
    border-color: var(--line-strong);
    background: var(--card-hover);
  }

  .ftab.universe.active {
    border-style: solid;
    border-color: var(--line-strong);
    background: var(--accent-dim);
    box-shadow: inset 0 -2px 0 var(--accent);
  }

  .ftab.add {
    width: 28px;
    padding: 0;
    justify-content: center;
    color: var(--faint);
    font-size: 15px;
  }

  .ftab.add.universe-add {
    font-size: 13px;
  }

  .ftab.add:hover {
    color: var(--text);
  }

  .icon-btn.subtle {
    color: var(--faint);
  }

  /* 数据状态 pill */
  .status-pill {
    height: 32px;
    max-width: 260px;
    display: inline-flex;
    align-items: center;
    gap: 7px;
    padding: 0 11px;
    color: var(--text);
    background: var(--panel-elev);
    border: 1px solid var(--line);
    border-radius: 999px;
    box-shadow: var(--shadow-sm);
    cursor: pointer;
    transition:
      border-color var(--transition-fast),
      background var(--transition-fast),
      box-shadow var(--transition-fast);
  }

  .status-pill:hover {
    background: var(--card-hover);
    border-color: var(--line-strong);
  }

  .status-pill .status-dot {
    background: var(--ok);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--ok) 18%, transparent);
  }

  .status-pill.busy .status-dot {
    background: var(--warn);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--warn) 18%, transparent);
  }

  .status-text {
    min-width: 0;
    overflow: hidden;
    font-size: 11px;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .status-action {
    flex: 0 0 auto;
    padding-left: 7px;
    color: var(--faint);
    border-left: 1px solid var(--line);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.1em;
  }

  .undo-group {
    display: inline-flex;
    align-items: center;
    background: var(--panel-elev);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    overflow: hidden;
  }

  .undo-group .icon-btn {
    width: 28px;
    height: 30px;
    border: none;
    border-radius: 0;
    font-size: 15px;
  }

  .undo-group .icon-btn + .icon-btn {
    border-left: 1px solid var(--line);
  }

  /* ── 菜单 ───────────────────────────────────────────────────── */

  .menu {
    position: absolute;
    z-index: 40;
    top: calc(100% + 6px);
    left: 0;
    min-width: 232px;
    max-height: min(520px, calc(100vh - 76px));
    overflow-y: auto;
    display: grid;
    gap: 1px;
    padding: 6px;
    background: var(--surface-glass);
    backdrop-filter: blur(12px);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius);
    box-shadow: var(--shadow-strong);
  }

  .menu.app-menu {
    left: auto;
    right: 0;
    width: 238px;
  }

  .menu.project-menu {
    width: 292px;
  }

  .menu.factory-menu {
    left: auto;
    right: 0;
    width: 220px;
  }

  .menu-label {
    padding: 6px 9px 4px;
    color: var(--faint);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.12em;
    text-transform: uppercase;
  }

  .menu button {
    width: 100%;
    min-height: 30px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 6px 9px;
    color: var(--text);
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    font-size: 11.5px;
    text-align: left;
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }

  .menu button:hover {
    background: var(--card-hover);
  }

  .menu button.active {
    background: var(--accent-dim);
    box-shadow: inset 2px 0 0 var(--accent);
  }

  .menu button.danger {
    color: var(--danger);
  }

  .menu button.danger:hover {
    background: var(--danger-dim);
  }

  .menu button:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .menu-item-main {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .menu-meta {
    flex: 0 0 auto;
    color: var(--faint);
    font-family: var(--mono);
    font-size: 9px;
  }

  .menu-empty {
    padding: 7px 8px;
    color: var(--faint);
    font-size: 11px;
  }

  .menu-sep {
    height: 1px;
    margin: 5px 4px;
    background: var(--line);
  }

  .menu-note {
    padding: 7px 8px 3px;
    margin-top: 4px;
    color: var(--faint);
    border-top: 1px solid var(--line);
    font-family: var(--mono);
    font-size: 9px;
    line-height: 1.5;
  }

  .theme-switch {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 2px;
    padding: 2px;
    background: var(--input-bg);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
  }

  .theme-switch button {
    min-height: 26px;
    justify-content: center;
    padding: 0 6px;
    color: var(--muted);
    font-size: 10.5px;
    text-align: center;
    transition: background var(--transition-fast), color var(--transition-fast);
  }

  .theme-switch button.active {
    color: var(--accent-ink);
    background: var(--accent);
    box-shadow: none;
    font-weight: 700;
  }

  @media (max-width: 1360px) {
    .factory-tabs {
      max-width: 34vw;
    }
    .status-text {
      max-width: 150px;
    }
  }

  @media (max-width: 1120px) {
    .trigger-label,
    .bar-label {
      display: none;
    }
    .project-trigger {
      max-width: 180px;
    }
  }
</style>
