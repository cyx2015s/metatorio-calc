<script lang="ts">
  // 纵向切片 demo：证明 runtime(Tauri) ↔ 前端全链路。
  // 必须在 Tauri 里跑（pnpm tauri dev）；纯浏览器没有 invoke。
  import { onMount } from "svelte";
  import { runtime } from "$lib/runtime/store.svelte.ts";
  import { dualVarLabel } from "$lib/runtime/types";
  import type { MechanicKind } from "$lib/runtime/types";

  const kinds: { kind: MechanicKind; label: string }[] = [
    { kind: "recipe", label: "Recipe" },
    { kind: "mining", label: "Mining" },
    { kind: "spoil", label: "Spoil" },
    { kind: "plant", label: "Plant" },
    { kind: "item-fuel", label: "ItemFuel" },
    { kind: "item-launch", label: "ItemLaunch" },
    { kind: "generator", label: "Generator" },
    { kind: "boiler", label: "Boiler" },
    { kind: "reactor", label: "Reactor" },
  ];

  let projectName = $state("Demo project");
  let factoryName = $state("Demo factory");
  let recipeId = $state("iron-gear-wheel");
  let machineId = $state("assembling-machine-1");
  let targetItemId = $state("iron-gear-wheel");
  let targetAmount = $state(1);
  let demoError = $state<string | null>(null);

  onMount(() => {
    runtime.init().catch(() => {});
  });

  let project = $derived(runtime.selectedProject);
  let factory = $derived(runtime.selectedFactory);
  let mechanics = $derived(factory?.mechanics ?? []);
  let targets = $derived(factory?.targets ?? []);
  const kindLabel = (kind: string) =>
    kinds.find((candidate) => candidate.kind === kind)?.label ?? kind;

  async function run(action: () => Promise<void>) {
    demoError = null;
    try {
      await action();
    } catch (error) {
      demoError = String(error);
    }
  }

  function oneClickDemo() {
    run(async () => {
      await runtime.loadDemoData();
      await runtime.newProject(projectName);
      await runtime.addFactory(factoryName);
      await runtime.addMechanic("recipe");
      const mechanic = runtime.selectedFactory?.mechanics.at(-1)?.id;
      if (mechanic == null) throw new Error("mechanic 创建失败");
      await runtime.setRecipe(mechanic, recipeId);
      await runtime.setMachine(mechanic, machineId);
      await runtime.addTarget(targetItemId, targetAmount);
      await runtime.recompute();
    });
  }
</script>

<svelte:head>
  <title>Metatorio / Runtime demo</title>
</svelte:head>

<main class="demo-shell">
  <header class="demo-header">
    <div>
      <div class="eyebrow">TAURI × SVELTE × METATORIO-RUNTIME</div>
      <h1>纵向切片闭环</h1>
    </div>
    <div class="header-status">
      <span class="chip" class:off={!runtime.ready}>RUNTIME {runtime.ready ? "ONLINE" : "OFFLINE"}</span>
      <span class="chip" class:off={!runtime.solving}>REV {runtime.revision}</span>
      {#if runtime.solving}<span class="chip warn">SOLVING…</span>{/if}
      {#if runtime.busy}<span class="chip warn">BUSY</span>{/if}
      <a class="quiet-link" href="/">设计原型 →</a>
    </div>
  </header>

  <section class="panel">
    <h2>步骤</h2>
    <div class="toolbar">
      <button onclick={() => run(() => runtime.loadDemoData())} disabled={runtime.busy}>加载示例数据</button>
      <button class="primary" onclick={oneClickDemo} disabled={runtime.busy || runtime.solving}>一键跑通示例</button>
      <span class="sep"></span>
      <input bind:value={projectName} placeholder="项目名" />
      <button onclick={() => run(() => runtime.newProject(projectName))} disabled={runtime.busy}>新建项目</button>
      <input bind:value={factoryName} placeholder="工厂名" />
      <button onclick={() => run(() => runtime.addFactory(factoryName))} disabled={runtime.busy}>新建工厂</button>
      <span class="sep"></span>
      <input bind:value={recipeId} placeholder="配方 id" />
      <input bind:value={machineId} placeholder="机器 id" />
      <input type="number" bind:value={targetAmount} min={0} step={0.1} />
      <button onclick={() => run(() => runtime.recompute())} disabled={runtime.busy || runtime.solving}>重新求解</button>
    </div>
    <div class="kind-row">
      {#each kinds as entry}
        <button onclick={() => run(() => runtime.addMechanic(entry.kind))} disabled={runtime.busy}>
          + {entry.label}
        </button>
      {/each}
    </div>
    {#if runtime.lastError || demoError || runtime.solveError}
      <div class="error-box">
        {#if runtime.lastError}<p>dispatch: {runtime.lastError}</p>{/if}
        {#if demoError}<p>demo: {demoError}</p>{/if}
        {#if runtime.solveError}<p>solve: {runtime.solveError}</p>{/if}
      </div>
    {/if}
  </section>

  <div class="columns">
    <section class="panel">
      <h2>文档快照</h2>
      {#if runtime.document}
        {#if project}
          <div class="card">
            <div class="card-title">项目 #{project.id}「{project.name}」</div>
            <div class="card-meta">工厂 {project.factories.length} 个 · 时间刻度 {project.settings.time_scale}</div>
            {#if factory}
              <div class="card-title sub">工厂 #{factory.id}「{factory.name}」</div>
              <div class="card-meta">
                机制 {mechanics.length} 个 · 目标 {targets.length} 个 · 严格输入 {factory.strict_source}
              </div>
            {:else}
              <p class="muted">尚未选中工厂。</p>
            {/if}
          </div>
        {:else}
          <p class="muted">尚无项目。点「新建项目」或「一键跑通示例」。</p>
        {/if}

        {#if mechanics.length > 0}
          <h3>机制</h3>
          <div class="mech-list">
            {#each mechanics as entry (entry.id)}
              <div class="mech-row">
                <span class="mech-id">#{entry.id}</span>
                <span class="chip">{kindLabel(entry.mechanic.type)}</span>
                <span class="mech-main">
                  <strong>{entry.mechanic.recipe?.id ?? entry.mechanic.resource ?? "—"}</strong>
                  <small>in {entry.mechanic.machine?.id ?? "—"}</small>
                </span>
                <span class="mech-enabled" class:off={!entry.enabled}>{entry.enabled ? "ON" : "OFF"}</span>
                <span class="mech-actions">
                  <button onclick={() => run(() => runtime.setRecipe(entry.id, recipeId))} disabled={runtime.busy}>配方</button>
                  <button onclick={() => run(() => runtime.setMachine(entry.id, machineId))} disabled={runtime.busy}>机器</button>
                </span>
              </div>
            {/each}
          </div>
        {/if}
      {:else}
        <p class="muted">正在连接后端…（需在 Tauri 窗口内运行）</p>
      {/if}
    </section>

    <section class="panel">
      <h2>目标</h2>
      {#if targets.length === 0}
        <p class="muted">还没有目标流。</p>
      {:else}
        <div class="mech-list">
          {#each targets as target (target.id)}
            <div class="target-row">
              <span class="chip cyan">{dualVarLabel(target.flow)}</span>
              <strong>{target.amount.toFixed(2)} /s</strong>
              <button onclick={() => run(() => runtime.addTarget(targetItemId, targetAmount))} disabled={runtime.busy}>追加</button>
            </div>
          {/each}
        </div>
      {/if}
      <div class="toolbar">
        <input bind:value={targetItemId} placeholder="目标物品 id" />
        <input type="number" bind:value={targetAmount} min={0} step={0.1} />
        <button onclick={() => run(() => runtime.addTarget(targetItemId, targetAmount))} disabled={runtime.busy}>
          添加目标
        </button>
      </div>

      <h2>求解结果</h2>
      {#if runtime.solve == null}
        <p class="muted">尚无求解结果。改完数据点「重新求解」。</p>
      {:else if "solved" in runtime.solve.status}
        {@const status = runtime.solve.status.solved}
        <div class="card">
          <div class="card-title">已求解 · 成本 {status.cost.toFixed(3)}</div>
          <div class="card-meta">机制 {status.mechanics.length} 个 · 流 {status.flows.length} 条</div>
        </div>
        <h3>机制解</h3>
        <div class="mech-list">
          {#each status.mechanics as solution (solution.mechanic)}
            <div class="target-row">
              <span class="chip">#{solution.mechanic}</span>
              <strong>{solution.amount.toFixed(4)}</strong>
              <span class="muted">variant {solution.variant}</span>
            </div>
          {/each}
        </div>
        <h3>流平衡</h3>
        <div class="mech-list">
          {#each status.flows as balance (balance.flow)}
            <div class="target-row">
              <span class="chip cyan">{dualVarLabel(balance.flow)}</span>
              <strong class:pos={balance.amount > 0}>{balance.amount > 0 ? "+" : ""}{balance.amount.toFixed(3)}</strong>
            </div>
          {/each}
        </div>
      {:else}
        {@const status = runtime.solve.status["not-solved"]}
        <div class="error-box">
          <p><strong>未求解</strong>：{status.description}</p>
          {#if status.no_provider.length > 0}
            <p>无供给：{status.no_provider.map(dualVarLabel).join(", ")}</p>
          {/if}
          {#if status.no_consumer.length > 0}
            <p>无消耗：{status.no_consumer.map(dualVarLabel).join(", ")}</p>
          {/if}
        </div>
      {/if}
    </section>
  </div>
</main>

<style>
  :global(body) {
    margin: 0;
    color: #e8ecea;
    background: #0d1012;
    font-family: "IBM Plex Sans", "Segoe UI", sans-serif;
    font-size: 13px;
  }

  .demo-shell {
    max-width: 1080px;
    margin: 0 auto;
    padding: 28px 30px 60px;
  }

  .demo-header {
    display: flex;
    justify-content: space-between;
    align-items: flex-end;
    gap: 18px;
    margin-bottom: 20px;
  }

  .eyebrow {
    color: #72807c;
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.13em;
  }

  h1 {
    margin: 8px 0 0;
    font-family: Georgia, serif;
    font-size: 30px;
    font-weight: 400;
    letter-spacing: -0.03em;
  }

  h2 {
    margin: 0 0 12px;
    font-family: Georgia, serif;
    font-size: 17px;
    font-weight: 400;
  }

  h3 {
    margin: 16px 0 8px;
    color: #b9c9c2;
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.12em;
    text-transform: uppercase;
  }

  .header-status {
    display: flex;
    align-items: center;
    gap: 8px;
    white-space: nowrap;
  }

  .chip {
    display: inline-block;
    padding: 4px 7px;
    color: #a1e2ce;
    background: #1d352d;
    border: 1px solid #3b6959;
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.08em;
  }

  .chip.off {
    color: #89938f;
    background: #1c2121;
    border-color: #33403d;
  }

  .chip.warn {
    color: #e3b873;
    background: #252016;
    border-color: #84673b;
  }

  .chip.cyan {
    color: #9bdcc6;
    background: #19372e;
    border-color: #2c5849;
  }

  .quiet-link {
    color: #7da996;
    font-size: 11px;
  }

  .panel {
    margin-bottom: 16px;
    padding: 18px;
    border: 1px solid #252d2d;
    background: #121718;
  }

  .toolbar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin-bottom: 10px;
  }

  .sep {
    width: 1px;
    height: 22px;
    margin: 0 4px;
    background: #2a3533;
  }

  button {
    min-height: 30px;
    padding: 0 10px;
    color: #cfe0d9;
    background: #1c2625;
    border: 1px solid #33413e;
    font: inherit;
    font-size: 11px;
    cursor: pointer;
  }

  button:hover {
    border-color: #6aab97;
    background: #22322e;
  }

  button:disabled {
    cursor: wait;
    opacity: 0.5;
  }

  button.primary {
    color: #10201a;
    background: #7ae2c0;
    border-color: #7ae2c0;
    font-weight: 800;
  }

  input {
    min-height: 30px;
    padding: 0 8px;
    color: #dde8e3;
    background: #0f1515;
    border: 1px solid #334944;
    font: inherit;
    font-size: 11px;
  }

  .kind-row {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .error-box {
    margin-top: 10px;
    padding: 10px 12px;
    color: #e0a6a0;
    background: #211819;
    border: 1px solid #4d3030;
    font-size: 11px;
  }

  .error-box p {
    margin: 4px 0;
  }

  .columns {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 16px;
    align-items: start;
  }

  .card {
    padding: 12px;
    background: #18201f;
    border: 1px solid #2a3432;
  }

  .card-title {
    color: #dce8e2;
    font-size: 12px;
    font-weight: 700;
  }

  .card-title.sub {
    margin-top: 10px;
    font-size: 11px;
  }

  .card-meta {
    margin-top: 4px;
    color: #788580;
    font-size: 10px;
  }

  .muted {
    color: #788580;
    font-size: 11px;
  }

  .mech-list {
    display: grid;
    gap: 6px;
  }

  .mech-row,
  .target-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    background: #161e1d;
    border: 1px solid #273130;
  }

  .mech-id {
    color: #70817b;
    font-family: Consolas, monospace;
    font-size: 10px;
  }

  .mech-main {
    display: grid;
    gap: 2px;
    min-width: 0;
  }

  .mech-main strong {
    overflow: hidden;
    color: #dce8e2;
    font-size: 11px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .mech-main small {
    color: #788580;
    font-size: 9px;
  }

  .mech-enabled {
    margin-left: auto;
    color: #a1e2ce;
    font-size: 8px;
    font-weight: 700;
  }

  .mech-enabled.off {
    color: #89938f;
  }

  .mech-actions {
    display: flex;
    gap: 4px;
  }

  .target-row strong {
    color: #d6e4dd;
    font-family: Consolas, monospace;
    font-size: 11px;
  }

  .target-row strong.pos {
    color: #87d5b9;
  }
</style>
