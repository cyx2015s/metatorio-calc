<script lang="ts">
  // Metatorio 主界面：紧凑布局，圆角卡片分区；游戏内物品一律图标按钮，
  // 一般操作一律文字按钮。
  import { onMount } from "svelte";
  import { runtime } from "$lib/runtime/store.svelte.ts";
  import { pickDumpFile, pickGameExecutable, pickModDir } from "$lib/runtime/client";
  import { dualVarLabel, itemOf } from "$lib/runtime/types";
  import type { CatalogKind, DualVar, MechanicId, TargetId } from "$lib/runtime/types";
  import Icon from "$lib/ui/Icon.svelte";
  import Selector from "$lib/ui/Selector.svelte";
  import MechanicCard from "$lib/ui/MechanicCard.svelte";

  const mechKinds: { kind: import("$lib/runtime/types").MechanicKind; label: string }[] = [
    { kind: "recipe", label: "配方" },
    { kind: "mining", label: "采矿" },
    { kind: "spoil", label: "腐坏" },
    { kind: "plant", label: "种植" },
    { kind: "item-fuel", label: "物品燃料" },
    { kind: "item-launch", label: "火箭发射" },
    { kind: "generator", label: "发电机" },
    { kind: "boiler", label: "锅炉" },
    { kind: "reactor", label: "反应堆" },
  ];

  onMount(() => {
    runtime.init().catch(() => {});
    runtime.clearCatalogCache();
  });

  // ── 应用栏状态 ──────────────────────────────────────────────────
  let dataMenuOpen = $state(false);
  // 添加机制菜单用 fixed 定位（视口坐标），脱离滚动容器，避免开合时
  // 改变机制列表滚动区域的高度。
  let addMechMenuPos = $state<{ top: number; right: number } | null>(null);
  let newProjectOpen = $state(false);
  let newProjectName = $state("新项目");
  let newFactoryOpen = $state(false);
  let newFactoryName = $state("新工厂");

  // ── 选择器状态 ──────────────────────────────────────────────────
  let selector = $state<{
    kind: CatalogKind;
    title: string;
    kinds: { kind: CatalogKind; label: string }[];
    onSelect: (name: string) => void;
  } | null>(null);

  function openSelector(
    kind: CatalogKind,
    title: string,
    onSelect: (name: string) => void,
    kinds: { kind: CatalogKind; label: string }[] = [],
  ) {
    selector = { kind, title, kinds, onSelect };
  }

  // ── 派生数据 ────────────────────────────────────────────────────
  let project = $derived(runtime.selectedProject);
  let factory = $derived(runtime.selectedFactory);
  let mechanics = $derived(factory?.mechanics ?? []);
  let targets = $derived(factory?.targets ?? []);
  let externalInputs = $derived(factory?.external_inputs ?? []);
  let solve = $derived(runtime.solve);
  let solveMap = $derived(
    new Map(
      (solve != null && "solved" in solve.status ? solve.status.solved.mechanics : []).map(
        (m) => [m.mechanic, m.amount] as const,
      ),
    ),
  );
  let solved = $derived(solve != null && "solved" in solve.status);
  let notSolved = $derived(solve != null && "not-solved" in solve.status);

  function flowIcon(flow: DualVar): { type: string; name: string } {
    const item = itemOf(flow);
    if (item) return { type: "item", name: item.id };
    if (flow !== null && typeof flow === "object" && "Fluid" in flow) {
      const fluid = (flow as { Fluid: { name: string } }).Fluid;
      return { type: "fluid", name: fluid.name };
    }
    return { type: "flow", name: dualVarLabel(flow) };
  }

  // ── 游戏数据加载 ────────────────────────────────────────────────
  async function loadFromExecutable() {
    dataMenuOpen = false;
    const exe = await pickGameExecutable();
    if (!exe) return;
    const modDir = await pickModDir();
    try {
      await runtime.loadContextFromExecutable(exe, modDir);
    } catch {
      /* contextError 已展示 */
    }
  }

  async function loadFromDump() {
    dataMenuOpen = false;
    const path = await pickDumpFile();
    if (!path) return;
    try {
      await runtime.loadContextFromDump(path);
    } catch {
      /* contextError 已展示 */
    }
  }

  async function loadDemo() {
    dataMenuOpen = false;
    try {
      await runtime.loadDemoData();
    } catch {
      /* contextError 已展示 */
    }
  }

  // ── 项目 / 工厂 ─────────────────────────────────────────────────
  async function createProject() {
    const name = newProjectName.trim();
    if (!name) return;
    newProjectOpen = false;
    try {
      await runtime.newProject(name);
    } catch {
      /* lastError 已展示 */
    }
  }

  async function createFactory() {
    const name = newFactoryName.trim();
    if (!name) return;
    newFactoryOpen = false;
    try {
      await runtime.addFactory(name);
    } catch {
      /* lastError 已展示 */
    }
  }

  // ── 机制拾取器分发 ──────────────────────────────────────────────
  function pickForMechanic(mechanic: MechanicId, kind: CatalogKind, slot?: number) {
    switch (kind) {
      case "recipe":
        openSelector("recipe", "选择配方", (name) => runtime.setRecipe(mechanic, name));
        break;
      case "resource":
        openSelector("resource", "选择资源", (name) => runtime.setResource(mechanic, name), [
          { kind: "resource", label: "资源" },
        ]);
        break;
      case "machine":
        openSelector("machine", "选择机器", (name) => runtime.setMachine(mechanic, name), [
          { kind: "machine", label: "制造机" },
          { kind: "mining-machine", label: "采矿机" },
          { kind: "beacon", label: "信标" },
        ]);
        break;
      case "mining-machine":
        openSelector("mining-machine", "选择采矿机", (name) => runtime.setMachine(mechanic, name));
        break;
      case "fluid":
        openSelector("fluid", "选择流体", (name) => runtime.setFluid(mechanic, name));
        break;
      case "item":
        openSelector("item", "选择物品", (name) => runtime.setItem(mechanic, name));
        break;
      case "generator":
        openSelector("generator", "选择发电机", (name) => runtime.setGenerator(mechanic, name));
        break;
      case "boiler":
        openSelector("boiler", "选择锅炉", (name) => runtime.setBoiler(mechanic, name));
        break;
      case "reactor":
        openSelector("reactor", "选择反应堆", (name) => runtime.setReactor(mechanic, name));
        break;
      case "module":
        openSelector("module", "选择模块", (name) => runtime.setModuleSlot(mechanic, slot ?? 0, name));
        break;
      default:
        break;
    }
  }
</script>

<svelte:head>
  <title>Metatorio</title>
</svelte:head>

<div class="app">
  <!-- ══ 应用栏 ══ -->
  <header class="appbar">
    <div class="brand">
      <span class="brand-mark">M</span>
      <span class="brand-name">METATORIO</span>
    </div>

    <div class="menu-wrap">
      <button class="btn" onclick={() => (dataMenuOpen = !dataMenuOpen)} disabled={runtime.contextBusy}>
        游戏数据{dataMenuOpen ? " ▴" : " ▾"}
      </button>
      {#if dataMenuOpen}
        <div class="menu">
          <button onclick={loadFromExecutable}>从游戏可执行文件加载…</button>
          <button onclick={loadFromDump}>从 data-raw-dump.json 加载…</button>
          <button onclick={loadDemo}>加载内置示例数据</button>
        </div>
      {/if}
    </div>

    {#if runtime.contextBusy}
      <span class="chip warn">正在加载数据…</span>
    {:else if runtime.context?.loaded}
      <span class="chip ok">数据已加载</span>
    {:else}
      <span class="chip">未加载游戏数据</span>
    {/if}

    <span class="spacer"></span>

    <button class="btn" onclick={() => (newProjectOpen = true)}>新建项目</button>
    <button class="btn" onclick={() => runtime.openProject().catch(() => {})} disabled={runtime.busy}>打开</button>
    <button class="btn" onclick={() => runtime.saveCurrentProject().catch(() => {})} disabled={runtime.busy}>
      保存{runtime.solving ? "（求解中）" : ""}
    </button>
  </header>

  {#if runtime.contextError || runtime.lastError}
    <div class="err-strip">
      {#if runtime.contextError}<span>数据：{runtime.contextError}</span>{/if}
      {#if runtime.lastError}<span>操作：{runtime.lastError}</span>{/if}
    </div>
  {/if}

  <!-- ══ 项目 / 工厂页签 ══ -->
  <nav class="tabs">
    {#each runtime.document?.projects ?? [] as item (item.id)}
      <button
        class:active={item.id === runtime.ui?.selected_project}
        class="tab"
        onclick={() => runtime.selectProject(item.id).catch(() => {})}
      >{item.name}</button>
    {/each}
    <button class="tab add" title="新建项目" onclick={() => (newProjectOpen = true)}>+</button>
  </nav>

  {#if project}
    <nav class="tabs sub">
      {#each project.factories as item (item.id)}
        <div class:active={item.id === runtime.ui?.selected_factory} class="tab-cluster">
          <button
            class="tab"
            onclick={() => runtime.selectFactory(item.id).catch(() => {})}
          >{item.name}</button>
          <button
            class="tab-x"
            title="删除工厂"
            onclick={() => runtime.removeFactory(item.id).catch(() => {})}
          >×</button>
        </div>
      {/each}
      <button class="tab add" title="新建工厂" onclick={() => (newFactoryOpen = true)}>+</button>
    </nav>
  {/if}

  <!-- ══ 工作区 ══ -->
  <main class="workspace">
    <!-- 左栏：目标 / 外部输入 / 项目设置 -->
    <aside class="col">
      {#if factory}
        <section class="panel">
          <div class="title">目标 <span class="count">{targets.length}</span></div>
          <div class="rows">
            {#each targets as target (target.id)}
              {@const icon = flowIcon(target.flow)}
              <div class="row-item">
                <Icon type={icon.type} name={icon.name} size={26} />
                <span class="row-name" title={dualVarLabel(target.flow)}>{dualVarLabel(target.flow)}</span>
                <input
                  class="num"
                  type="number"
                  step="0.1"
                  min="0"
                  value={String(target.amount)}
                  onchange={(event) => {
                    const value = Number((event.currentTarget as HTMLInputElement).value);
                    if (Number.isFinite(value)) runtime.setTargetAmount(target.id, value).catch(() => {});
                  }}
                />
                <button class="btn ghost" title="移除目标" onclick={() => runtime.removeTarget(target.id).catch(() => {})}>×</button>
              </div>
            {:else}
              <div class="empty-hint">还没有目标流</div>
            {/each}
          </div>
          <button
            class="btn"
            onclick={() =>
              openSelector("item", "选择目标物品", (name) =>
                runtime.addTarget(name, 1).catch(() => {}),
              )}
            disabled={!runtime.context?.loaded}
          >+ 添加目标</button>
        </section>

        <section class="panel">
          <div class="title">外部输入 <span class="count">{externalInputs.length}</span></div>
          <div class="rows">
            {#each externalInputs as input (input.id)}
              {@const icon = flowIcon(input.flow)}
              <div class="row-item">
                <Icon type={icon.type} name={icon.name} size={26} />
                <span class="row-name" title={dualVarLabel(input.flow)}>{dualVarLabel(input.flow)}</span>
                <input
                  class="num"
                  type="number"
                  step="0.1"
                  min="0"
                  value={String(input.penalty)}
                  onchange={(event) => {
                    const value = Number((event.currentTarget as HTMLInputElement).value);
                    if (Number.isFinite(value)) runtime.setExternalInputPenalty(input.id, value).catch(() => {});
                  }}
                />
                <button class="btn ghost" title="移除" onclick={() => runtime.removeExternalInput(input.id).catch(() => {})}>×</button>
              </div>
            {:else}
              <div class="empty-hint">还没有外部输入</div>
          {/each}
        </div>
        <button
          class="btn"
          onclick={() =>
            openSelector("item", "选择外部输入物品", (name) =>
              runtime.addExternalInput(name, 1).catch(() => {}),
            )}
          disabled={!runtime.context?.loaded}
        >+ 添加外部输入</button>
        </section>
      {/if}

      {#if project}
        <section class="panel">
          <div class="title">项目设置</div>
          <div class="field">
            <label>时间刻度</label>
            <select
              value={project.settings.time_scale}
              onchange={(event) =>
                runtime
                  .setTimeScale((event.currentTarget as HTMLSelectElement).value as import("$lib/runtime/types").TimeScale)
                  .catch(() => {})}
            >
              <option value="seconds">秒</option>
              <option value="minutes">分钟</option>
              <option value="hours">小时</option>
            </select>
          </div>
          <label class="check">
            <input
              type="checkbox"
              checked={project.settings.all_accessible}
              onchange={(event) =>
                runtime.setAllAccessible((event.currentTarget as HTMLInputElement).checked).catch(() => {})}
            />
            全部科技默认解锁
          </label>
          <div class="field">
            <label>采矿产出加成（倍率）</label>
            <input
              type="number"
              step="0.1"
              min="0"
              value={String(project.settings.mining_productivity)}
              onchange={(event) => {
                const value = Number((event.currentTarget as HTMLInputElement).value);
                if (Number.isFinite(value)) runtime.setMiningProductivity(value).catch(() => {});
              }}
            />
          </div>
        </section>
      {/if}
    </aside>

    <!-- 中栏：机制列表 -->
    <section class="col center">
      <div class="toolbar">
        <button class="btn primary" onclick={() => runtime.recompute().catch(() => {})} disabled={runtime.busy || runtime.solving || !factory}>
          {runtime.solving ? "求解中…" : "重新求解"}
        </button>
        {#if runtime.context?.loaded == null || !runtime.context.loaded}
          <span class="muted">先加载游戏数据（左上角「游戏数据」）</span>
        {/if}
        <span class="spacer"></span>
        <span class="chip">{mechanics.length} 机制</span>
        <span class="chip">{targets.length} 目标</span>
        {#if solved}<span class="chip ok">已求解</span>{/if}
        {#if notSolved}<span class="chip warn">未求解</span>{/if}
        {#if runtime.solveError}<span class="chip warn">求解错误</span>{/if}
      </div>

      <div class="mech-list">
        {#each mechanics as entry (entry.id)}
          <MechanicCard
            {entry}
            amount={solveMap.get(entry.id) ?? null}
            onPick={(kind, slot) => pickForMechanic(entry.id, kind, slot)}
            onToggleEnabled={() => runtime.setMechanicEnabled(entry.id, !entry.enabled).catch(() => {})}
            onRemove={() => runtime.removeMechanic(entry.id).catch(() => {})}
            onModuleSlot={(slot, module) => runtime.setModuleSlot(entry.id, slot, module).catch(() => {})}
          />
        {:else}
          <div class="empty-state">
            {#if factory}
              <span class="muted">还没有机制，点下方「添加机制」</span>
            {:else}
              <span class="muted">选择或新建一个工厂</span>
            {/if}
          </div>
        {/each}
      </div>

      {#if factory}
        <div class="add-wrap">
          <button
            class="btn"
            onclick={(event) => {
              const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
              addMechMenuPos = addMechMenuPos
                ? null
                : { top: rect.bottom + 4, right: Math.max(8, window.innerWidth - rect.right) };
            }}
          >
            + 添加机制{addMechMenuPos ? " ▴" : " ▾"}
          </button>
        </div>
      {/if}
    </section>

    {#if addMechMenuPos}
      <div class="menu-catcher" aria-hidden="true" onclick={() => (addMechMenuPos = null)}></div>
      <div class="menu fixed" style={`top:${addMechMenuPos.top}px;right:${addMechMenuPos.right}px`}>
        {#each mechKinds as option (option.kind)}
          <button
            onclick={() => {
              addMechMenuPos = null;
              runtime.addMechanic(option.kind).catch(() => {});
            }}
          >{option.label}</button>
        {/each}
      </div>
    {/if}

    <!-- 右栏：求解结果 / 数据上下文 -->
    <aside class="col">
      <section class="panel">
        <div class="title">求解结果</div>
        {#if solve && "solved" in solve.status}
          {@const status = solve.status.solved}
          <div class="kv">
            <span>总成本</span><strong class="mono">{status.cost.toFixed(3)}</strong>
          </div>
          <div class="subtitle">机制解</div>
          <div class="rows compact">
            {#each status.mechanics as m (m.mechanic)}
              <div class="row-item">
                <span class="row-name mono">#{m.mechanic}</span>
                <strong class="mono amount-pos">{m.amount.toFixed(4)}</strong>
              </div>
            {/each}
          </div>
          <div class="subtitle">流平衡</div>
          <div class="rows compact">
            {#each status.flows as balance (balance.flow)}
              {@const icon = flowIcon(balance.flow)}
              <div class="row-item">
                <Icon type={icon.type} name={icon.name} size={22} />
                <span class="row-name" title={dualVarLabel(balance.flow)}>{dualVarLabel(balance.flow)}</span>
                <strong class:amount-pos={balance.amount > 0} class="mono amount">{balance.amount > 0 ? "+" : ""}{balance.amount.toFixed(3)}</strong>
              </div>
            {/each}
          </div>
        {:else if solve && "not-solved" in solve.status}
          {@const status = solve.status["not-solved"]}
          <div class="err-box">
            <div><strong>未求解</strong>：{status.description}</div>
            {#if status.no_provider.length > 0}
              <div>无供给：{status.no_provider.map(dualVarLabel).join(", ")}</div>
            {/if}
            {#if status.no_consumer.length > 0}
              <div>无消耗：{status.no_consumer.map(dualVarLabel).join(", ")}</div>
            {/if}
          </div>
        {:else if runtime.solveError}
          <div class="err-box">{runtime.solveError}</div>
        {:else}
          <div class="empty-hint">改完数据后点「重新求解」</div>
        {/if}
      </section>

      <section class="panel">
        <div class="title">数据上下文</div>
        {#if runtime.context?.loaded}
          <div class="kv"><span>图标目录</span><span class="mono small" title={runtime.context.icon_root ?? ""}>{runtime.context.icon_root ?? "无（仅占位图标）"}</span></div>
          <div class="chips-wrap">
            {#each runtime.context.groups.filter((group) => group.count > 0) as group (group.name)}
              <span class="chip" title={group.name}>{group.name}·{group.count}</span>
            {/each}
          </div>
        {:else}
          <div class="empty-hint">未加载游戏数据</div>
        {/if}
      </section>
    </aside>
  </main>
</div>

<!-- ══ 选择器 ══ -->
{#if selector}
  <Selector
    kind={selector.kind}
    title={selector.title}
    kindOptions={selector.kinds}
    onSelect={selector.onSelect}
    onClose={() => (selector = null)}
  />
{/if}

<!-- ══ 新建项目 / 新建工厂 ══ -->
{#if newProjectOpen}
  <div class="backdrop" onclick={() => (newProjectOpen = false)}>
    <div class="mini-modal" onclick={(event) => event.stopPropagation()}>
      <div class="mini-title">新建项目</div>
      <input
        bind:value={newProjectName}
        onkeydown={(event) => {
          if (event.key === "Enter") createProject();
        }}
      />
      <div class="mini-actions">
        <button class="btn ghost" onclick={() => (newProjectOpen = false)}>取消</button>
        <button class="btn primary" onclick={createProject}>创建</button>
      </div>
    </div>
  </div>
{/if}

{#if newFactoryOpen}
  <div class="backdrop" onclick={() => (newFactoryOpen = false)}>
    <div class="mini-modal" onclick={(event) => event.stopPropagation()}>
      <div class="mini-title">新建工厂</div>
      <input
        bind:value={newFactoryName}
        onkeydown={(event) => {
          if (event.key === "Enter") createFactory();
        }}
      />
      <div class="mini-actions">
        <button class="btn ghost" onclick={() => (newFactoryOpen = false)}>取消</button>
        <button class="btn primary" onclick={createFactory}>创建</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }

  .appbar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    background: var(--panel);
    border-bottom: 1px solid var(--line);
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-right: 6px;
  }

  .brand-mark {
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    color: #0f1f19;
    background: var(--accent);
    border-radius: 6px;
    font-size: 11px;
    font-weight: 800;
  }

  .brand-name {
    font-size: 11px;
    font-weight: 800;
    letter-spacing: 0.14em;
  }

  .spacer {
    flex: 1;
  }

  .menu-wrap {
    position: relative;
  }

  .menu {
    position: absolute;
    z-index: 30;
    top: calc(100% + 6px);
    left: 0;
    min-width: 210px;
    padding: 5px;
    display: grid;
    gap: 2px;
    background: var(--panel);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius);
    box-shadow: 0 12px 30px rgba(0, 0, 0, 0.45);
  }

  .menu button {
    padding: 7px 9px;
    text-align: left;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    font-size: 11px;
    cursor: pointer;
  }

  .menu button:hover {
    background: var(--card-hover);
  }

  .add-wrap {
    padding-top: 2px;
  }

  /* fixed 定位的弹出菜单：完全脱离布局，不参与任何滚动区域 */
  .menu-catcher {
    position: fixed;
    z-index: 29;
    inset: 0;
  }

  .menu.fixed {
    position: fixed;
    z-index: 30;
    min-width: 190px;
    max-height: min(420px, 60vh);
    overflow-y: auto;
    padding: 5px;
    display: grid;
    gap: 2px;
    background: var(--panel);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius);
    box-shadow: 0 12px 30px rgba(0, 0, 0, 0.45);
  }

  .err-strip {
    display: flex;
    flex-wrap: wrap;
    gap: 14px;
    padding: 6px 12px;
    color: var(--danger);
    background: var(--danger-dim);
    border-bottom: 1px solid var(--danger-line);
    font-size: 11px;
  }

  .tabs {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 6px 12px 0;
    background: var(--panel);
    border-bottom: 1px solid var(--line);
  }

  .tabs.sub {
    background: var(--bg);
  }

  .tab {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 6px 10px;
    color: var(--muted);
    background: transparent;
    border: 1px solid transparent;
    border-bottom: none;
    border-radius: var(--radius-sm) var(--radius-sm) 0 0;
    font-size: 11px;
    cursor: pointer;
    max-width: 180px;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .tab:hover {
    color: var(--text);
  }

  .tab.active {
    color: var(--text);
    background: var(--bg);
    border-color: var(--line);
  }

  .tab-cluster {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    background: transparent;
    border: 1px solid transparent;
    border-bottom: none;
    border-radius: var(--radius-sm) var(--radius-sm) 0 0;
    max-width: 200px;
  }

  .tab-cluster.active {
    background: var(--bg);
    border-color: var(--line);
  }

  .tab-cluster .tab {
    max-width: 160px;
    border: none;
    border-radius: 0;
  }

  .tab-cluster .tab:hover {
    background: transparent;
  }

  .tabs:not(.sub) .tab.active {
    background: var(--bg);
  }

  .tab.add {
    color: var(--faint);
    font-size: 14px;
  }

  .tab-x {
    padding: 0 2px;
    color: var(--faint);
    background: transparent;
    border: none;
    font-size: 12px;
    cursor: pointer;
  }

  .tab-x:hover {
    color: var(--danger);
  }

  .workspace {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: 250px minmax(420px, 1fr) 290px;
    gap: 0;
    padding: 10px 0;
    overflow: hidden;
  }

  .col {
    min-height: 0;
    overflow-y: auto;
    display: grid;
    align-content: start;
    gap: 10px;
    padding: 0 12px;
  }

  /* 分区竖分隔线 */
  .col + .col {
    border-left: 1px solid var(--line);
  }

  .rows {
    display: grid;
    gap: 4px;
    margin-bottom: 8px;
  }

  .rows.compact {
    gap: 2px;
  }

  .row-item {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 32px;
    padding: 3px 6px;
    background: var(--card);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
  }

  .row-name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    font-size: 11px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .num {
    width: 68px;
    min-height: 24px;
    padding: 0 6px;
    text-align: right;
    background: var(--bg);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-sm);
    font-family: var(--mono);
    font-size: 10px;
  }

  .empty-hint {
    padding: 14px 4px;
    color: var(--faint);
    font-size: 11px;
  }

  .center {
    display: flex;
    flex-direction: column;
  }

  .toolbar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding-bottom: 8px;
    margin-bottom: 8px;
    border-bottom: 1px solid var(--line);
  }

  .mech-list {
    display: grid;
    align-content: start;
    gap: 8px;
  }

  .empty-state {
    padding: 30px;
    text-align: center;
    border: 1px dashed var(--line-strong);
    border-radius: var(--radius);
  }

  .kv {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
    padding: 4px 0;
    color: var(--muted);
    font-size: 11px;
  }

  .kv strong {
    color: var(--text);
  }

  .kv .small {
    overflow: hidden;
    max-width: 180px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .subtitle {
    margin: 10px 0 5px;
    color: var(--muted);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.1em;
    text-transform: uppercase;
  }

  .amount {
    font-size: 10px;
  }

  .amount-pos {
    color: var(--accent);
  }

  .chips-wrap {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin-top: 8px;
  }

  .check {
    display: flex;
    align-items: center;
    gap: 7px;
    margin: 10px 0;
    color: var(--text);
    font-size: 11px;
    cursor: pointer;
  }

  .backdrop {
    position: fixed;
    z-index: 50;
    inset: 0;
    display: grid;
    place-items: center;
    padding: 24px;
    background: rgba(4, 7, 8, 0.72);
  }

  .mini-modal {
    width: min(340px, 100%);
    display: grid;
    gap: 10px;
    padding: 14px;
    background: var(--panel);
    border: 1px solid var(--accent-line);
    border-radius: var(--radius);
  }

  .mini-title {
    font-size: 12px;
    font-weight: 700;
  }

  .mini-modal input {
    min-height: 30px;
    padding: 0 9px;
    background: var(--bg);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-sm);
    font-size: 12px;
  }

  .mini-actions {
    display: flex;
    justify-content: flex-end;
    gap: 6px;
  }
</style>
