<script lang="ts">
  import { runtime } from "$lib/runtime/store.svelte";
  import type { IdWithQuality, Location, MechanicKind, UniverseDocument } from "$lib/runtime/types";

  let { universe }: { universe: UniverseDocument } = $props();
  let locationMode = $state<"planet" | "space">("planet");
  let planet = $state("nauvis");
  let mechanicKind = $state<MechanicKind>("recipe");
  let transferPlanet = $state("nauvis");
  let transferItem = $state("iron-plate");
  let transferQuality = $state("normal");
  let targetItem = $state("iron-plate");
  let targetQuality = $state("normal");
  let targetAmount = $state(1);
  let targetMode = $state<"planet" | "space">("planet");
  let targetPlanet = $state("nauvis");
  let exprMode = $state<"planet" | "space">("planet");
  let exprPlanet = $state("nauvis");
  let viewMode = $state<"list" | "network">("list");
  let query = $state("");
  let solving = $state(false);

  const item = (id: string, quality: string): IdWithQuality => ({ id: id.trim(), quality: quality.trim() || "normal" });
  const projectQuality = () => runtime.selectedProject?.settings.major_quality || "normal";
  const location = (mode: "planet" | "space", name: string): Location =>
    mode === "space" ? { kind: "space" } : { kind: "planet", name: name.trim() || "nauvis" };
  const locationLabel = (value: Location) => value.kind === "space" ? "太空" : value.name;
  const presentFlow = (flow: unknown) => {
    if (typeof flow === "string") return flow;
    if (flow && typeof flow === "object" && "Item" in flow) return (flow as { Item: IdWithQuality }).Item.id;
    if (flow && typeof flow === "object" && "Fluid" in flow) return (flow as { Fluid: { name: string } }).Fluid.name;
    return JSON.stringify(flow);
  };

  async function solve(auto = false) {
    if (solving) return;
    solving = true;
    try {
      if (auto) await runtime.autoPlanSelectedUniverse();
      else await runtime.solveSelectedUniverse();
    } finally {
      solving = false;
    }
  }

  let drafts = $state<Record<number, { a: string; b: string }>>({});
  const draft = (id: number) => (drafts[id] ??= { a: "", b: "" });
  const localMechanic = (id: number) => {
    const entry = universe.mechanics.find((candidate) => candidate.id === id);
    return entry?.mechanic.type === "local" ? entry.mechanic.mechanic : null;
  };
  const machineQuality = (id: number, key: "recipe" | "mining") => {
    const mechanic = localMechanic(id);
    if (key === "recipe" && mechanic?.type === "recipe") return mechanic.machine?.quality || "normal";
    if (key === "mining" && mechanic?.type === "mining") return mechanic.machine?.quality || "normal";
    return "normal";
  };
  const applyRecipe = async (id: number) => {
    const d = draft(id);
    if (d.a.trim()) await runtime.updateUniverseMechanic(id, { recipe: { "set-recipe": { recipe: item(d.a, projectQuality()) } } });
    if (d.b.trim()) await runtime.updateUniverseMechanic(id, { recipe: { "set-machine": { machine: item(d.b, machineQuality(id, "recipe")) } } });
  };
  const applyMining = async (id: number) => {
    const d = draft(id);
    if (d.a.trim()) await runtime.updateUniverseMechanic(id, { mining: { "set-resource": { resource: d.a.trim() } } });
    if (d.b.trim()) await runtime.updateUniverseMechanic(id, { mining: { "set-machine": { machine: item(d.b, machineQuality(id, "mining")) } } });
  };
  const applyItemFuel = (id: number) => {
    const d = draft(id);
    if (d.a.trim()) runtime.updateUniverseMechanic(id, { "item-fuel": { "set-item": { item: item(d.a, projectQuality()) } } }).catch(() => {});
  };

  const kinds: MechanicKind[] = [
    "recipe", "mining", "spoil", "plant", "item-fuel", "item-launch", "generator", "boiler",
    "reactor", "solar", "fluid-fuel", "fluid-heat", "tile-extract", "tile-dispose", "virtual-science",
  ];

  let filteredMechanics = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (!q) return universe.mechanics;
    return universe.mechanics.filter((entry) => {
      const loc = locationLabel(entry.mechanic.location);
      const type = entry.mechanic.type === "transfer" ? "transfer" : entry.mechanic.mechanic.type;
      const id = entry.mechanic.type === "transfer" ? entry.mechanic.item.id : "";
      return loc.toLowerCase().includes(q) || type.toLowerCase().includes(q) || id.toLowerCase().includes(q);
    });
  });
  let networkGroups = $derived.by(() => {
    const groups = new Map<string, typeof universe.mechanics>();
    for (const entry of filteredMechanics) {
      const key = locationLabel(entry.mechanic.location);
      const list = groups.get(key) ?? [];
      list.push(entry);
      groups.set(key, list);
    }
    return [...groups.entries()];
  });
</script>

<div class="universe-page">
  <header>
    <div class="title">
      <strong>宇宙工厂 · {universe.name}</strong>
      <span>{universe.mechanics.length} 个机制 · {universe.targets.length} 个目标 · {universe.target_expressions.length} 个表达式</span>
    </div>
    <div class="actions">
      <button class="btn ghost" disabled={solving} onclick={() => solve(true)}>{solving ? "处理中…" : "自动规划"}</button>
      <button class="btn" disabled={solving} onclick={() => solve()}>{solving ? "求解中…" : "求解"}</button>
      <button class="btn ghost" onclick={() => runtime.removeUniverse(universe.id).catch(() => {})}>删除</button>
      <button class="btn ghost" onclick={() => runtime.selectUniverse(null)}>关闭</button>
    </div>
  </header>

  <div class="page-body">
    <aside class="rail">
      <section>
        <h3>目标</h3>
        <div class="stack">
          <select bind:value={targetMode}>
            <option value="planet">星球</option>
            <option value="space">太空</option>
          </select>
          {#if targetMode === "planet"}<input bind:value={targetPlanet} placeholder="星球名" />{/if}
          <input bind:value={targetItem} placeholder="物品 id" />
          <input bind:value={targetQuality} placeholder="品质" />
          <input type="number" bind:value={targetAmount} />
          <button class="btn" onclick={() => runtime.addUniverseTarget({ location: location(targetMode, targetPlanet), flow: { Item: item(targetItem, targetQuality) } }, targetAmount).catch(() => {})}>添加目标</button>
        </div>
        {#each universe.targets as target (target.id)}
          <div class="entry">
            <span class="pill">{locationLabel(target.flow.location)}</span>
            <span class="grow">{presentFlow(target.flow.flow)} ×</span>
            <input type="number" value={target.amount} onchange={(event) => runtime.setUniverseTargetAmount(target.id, Number(event.currentTarget.value)).catch(() => {})} />
            <button class="icon" title="删除目标" onclick={() => runtime.removeUniverseTarget(target.id).catch(() => {})}>×</button>
          </div>
        {/each}
      </section>

      <section>
        <h3>目标表达式</h3>
        <div class="stack">
          <select bind:value={exprMode}>
            <option value="planet">项地点：星球</option>
            <option value="space">项地点：太空</option>
          </select>
          {#if exprMode === "planet"}<input bind:value={exprPlanet} placeholder="星球名" />{/if}
          <input bind:value={targetItem} placeholder="项物品 id" />
          <input bind:value={targetQuality} placeholder="项品质" />
          <button class="btn" onclick={() => runtime.addUniverseTargetExpression().catch(() => {})}>+ 表达式</button>
        </div>
        {#each universe.target_expressions as expression (expression.id)}
          <div class="expr">
            <div class="entry">
              <span>常数</span>
              <input type="number" value={expression.constant} onchange={(event) => runtime.setUniverseTargetExpressionConstant(expression.id, Number(event.currentTarget.value)).catch(() => {})} />
              <button class="icon" title="删除表达式" onclick={() => runtime.removeUniverseTargetExpression(expression.id).catch(() => {})}>×</button>
            </div>
            {#each expression.terms as term (term.id)}
              <div class="entry term-row">
                <span class="pill">{locationLabel(term.flow.location)}</span>
                <span class="grow">{presentFlow(term.flow.flow)}</span>
                <input type="number" value={term.coefficient} onchange={(event) => runtime.setUniverseTargetExpressionTermCoefficient(expression.id, term.id, Number(event.currentTarget.value)).catch(() => {})} />
                <button class="icon" title="移除项" onclick={() => runtime.removeUniverseTargetExpressionTerm(expression.id, term.id).catch(() => {})}>×</button>
              </div>
            {/each}
            <button class="btn ghost" onclick={() => runtime.addUniverseTargetExpressionTerm(expression.id, { location: location(exprMode, exprPlanet), flow: { Item: item(targetItem, targetQuality) } }).catch(() => {})}>+ 物品项</button>
          </div>
        {/each}
      </section>
    </aside>

    <main class="center">
      <div class="section-head">
        <h3>机制 <span class="count">{filteredMechanics.length}/{universe.mechanics.length}</span></h3>
        <div class="row">
          <input bind:value={query} placeholder="筛选地点 / 类型 / 物品" />
          <button class="btn ghost" class:active={viewMode === "list"} onclick={() => (viewMode = "list")}>列表</button>
          <button class="btn ghost" class:active={viewMode === "network"} onclick={() => (viewMode = "network")}>网络</button>
        </div>
      </div>

      <div class="add-bar">
        <div class="row">
          <select bind:value={locationMode}>
            <option value="planet">星球</option>
            <option value="space">太空</option>
          </select>
          {#if locationMode === "planet"}<input bind:value={planet} placeholder="星球名" />{/if}
          <select bind:value={mechanicKind}>
            {#each kinds as kind}<option value={kind}>{kind}</option>{/each}
          </select>
          <button class="btn" onclick={() => runtime.addUniverseLocal(location(locationMode, planet), mechanicKind).catch(() => {})}>+ 本地机制</button>
        </div>
        <div class="row">
          <span class="pill">火箭转移</span>
          <input bind:value={transferPlanet} placeholder="源星球" />
          <input bind:value={transferItem} placeholder="物品 id" />
          <input bind:value={transferQuality} placeholder="品质" />
          <button class="btn" onclick={() => runtime.addUniverseTransfer(location("planet", transferPlanet), item(transferItem, transferQuality)).catch(() => {})}>+ 转移</button>
        </div>
      </div>

      {#if viewMode === "network"}
        <div class="network-view">
          {#each networkGroups as [locationName, entries] (locationName)}
            <div class="network-lane">
              <h4>{locationName} <span class="count">{entries.length}</span></h4>
              {#each entries as entry (entry.id)}
                <div class="network-card">
                  <span>{entry.mechanic.type === "transfer" ? `转移 ${entry.mechanic.item.id}` : entry.mechanic.mechanic.type}</span>
                  <span class="muted">{entry.enabled ? "" : "已禁用"}</span>
                </div>
              {/each}
            </div>
          {/each}
        </div>
      {:else}
      <div class="entries">
        {#each filteredMechanics as entry (entry.id)}
          <div class="entry mechanic">
            {#if entry.mechanic.type === "local"}
              <select
                value={entry.mechanic.location.kind === "space" ? "space" : "planet"}
                onchange={(event) => runtime.setUniverseMechanicLocation(entry.id, event.currentTarget.value === "space"
                  ? { kind: "space" }
                  : { kind: "planet", name: entry.mechanic.location.kind === "planet" ? entry.mechanic.location.name : "nauvis" }).catch(() => {})}
              >
                <option value="planet">星球</option>
                <option value="space">太空</option>
              </select>
            {:else}
              <span class="pill">星球</span>
            {/if}
            {#if entry.mechanic.location.kind === "planet"}
              <input
                value={entry.mechanic.location.name}
                placeholder="星球名"
                onchange={(event) => runtime.setUniverseMechanicLocation(entry.id, { kind: "planet", name: event.currentTarget.value.trim() || "nauvis" }).catch(() => {})}
              />
            {/if}
            <span>{entry.mechanic.type === "transfer" ? `转移 ${entry.mechanic.item.id}` : entry.mechanic.mechanic.type}</span>
            {#if entry.mechanic.type === "local" && entry.mechanic.mechanic.type === "recipe"}
              <input value={drafts[entry.id]?.a ?? ""} oninput={(event) => (drafts[entry.id] = { ...draft(entry.id), a: event.currentTarget.value })} placeholder={entry.mechanic.mechanic.recipe?.id || "配方 id"} />
              <input value={drafts[entry.id]?.b ?? ""} oninput={(event) => (drafts[entry.id] = { ...draft(entry.id), b: event.currentTarget.value })} placeholder={entry.mechanic.mechanic.machine?.id || "机器 id"} />
              <button class="btn ghost" onclick={() => applyRecipe(entry.id)}>应用配方</button>
            {:else if entry.mechanic.type === "local" && entry.mechanic.mechanic.type === "mining"}
              <input value={drafts[entry.id]?.a ?? ""} oninput={(event) => (drafts[entry.id] = { ...draft(entry.id), a: event.currentTarget.value })} placeholder="资源 id" />
              <input value={drafts[entry.id]?.b ?? ""} oninput={(event) => (drafts[entry.id] = { ...draft(entry.id), b: event.currentTarget.value })} placeholder="采矿机 id" />
              <button class="btn ghost" onclick={() => applyMining(entry.id)}>应用采矿</button>
            {:else if entry.mechanic.type === "local" && entry.mechanic.mechanic.type === "item-fuel"}
              <input value={drafts[entry.id]?.a ?? ""} oninput={(event) => (drafts[entry.id] = { ...draft(entry.id), a: event.currentTarget.value })} placeholder="燃料物品 id" />
              <button class="btn ghost" onclick={() => applyItemFuel(entry.id)}>应用燃料</button>
            {/if}
            {#if entry.mechanic.type === "local"}
              <details class="json-editor">
                <summary>高级 JSON</summary>
                <textarea value={JSON.stringify(entry.mechanic.mechanic)} onchange={(event) => runtime.setUniverseMechanicJson(entry.id, event.currentTarget.value).catch(() => {})}></textarea>
              </details>
            {/if}
            <button class="btn ghost" onclick={() => runtime.setUniverseMechanicEnabled(entry.id, !entry.enabled).catch(() => {})}>{entry.enabled ? "禁用" : "启用"}</button>
            <button class="icon" title="删除机制" onclick={() => runtime.removeUniverseMechanic(entry.id).catch(() => {})}>×</button>
          </div>
        {/each}
      </div>
      {/if}
    </main>

    <aside class="results">
      <h3>求解结果</h3>
      {#if runtime.universeSolveError}<p class="error">{runtime.universeSolveError}</p>{/if}
      {#if runtime.universeSolve}
        {#if runtime.universeSolve.status.status === "solved"}
          <p>成本 {runtime.universeSolve.status.cost}</p>
          <h4>机制用量</h4>
          {#each runtime.universeSolve.status.mechanics as solution (solution.mechanic)}
            <div class="entry"><span class="pill">{locationLabel(solution.location)}</span><span class="grow">#{solution.mechanic}</span><span>{solution.amount}</span></div>
          {/each}
          <h4>流量</h4>
          {#each runtime.universeSolve.status.flows as flow (JSON.stringify(flow.flow))}
            <div class="entry"><span class="pill">{locationLabel(flow.flow.location)}</span><span class="grow">{presentFlow(flow.flow.flow)}</span><span>{flow.amount}</span></div>
          {/each}
        {:else}
          <p class="error">{runtime.universeSolve.status.description}</p>
        {/if}
      {/if}
    </aside>
  </div>
</div>

<style>
  .universe-page { position: absolute; inset: 0; z-index: 10; display: flex; flex-direction: column; background: var(--bg); color: var(--text); }
  header { display: flex; justify-content: space-between; gap: 12px; align-items: center; padding: 10px 14px; border-bottom: 1px solid var(--line); flex: 0 0 auto; }
  .title { display: flex; flex-direction: column; gap: 2px; }
  .title span { color: var(--muted); font-size: 12px; }
  .actions, .row { display: flex; gap: 6px; flex-wrap: wrap; align-items: center; }
  .page-body { flex: 1 1 auto; min-height: 0; display: grid; grid-template-columns: 300px minmax(0, 1fr) 310px; }
  .rail, .results { overflow: auto; padding: 12px; border-right: 1px solid var(--line); display: grid; align-content: start; gap: 16px; }
  .results { border-right: 0; border-left: 1px solid var(--line); }
  .center { overflow: auto; padding: 12px; display: flex; flex-direction: column; gap: 10px; min-width: 0; }
  section { display: grid; gap: 8px; }
  h3, h4 { margin: 0; font-size: 13px; }
  .stack { display: grid; gap: 5px; }
  .section-head { display: flex; justify-content: space-between; gap: 10px; align-items: center; flex-wrap: wrap; }
  .count { color: var(--muted); font-weight: normal; }
  .add-bar { display: grid; gap: 6px; padding: 8px 0; border-top: 1px solid var(--line); border-bottom: 1px solid var(--line); }
  input, select { background: var(--bg); color: var(--text); border: 1px solid var(--line); border-radius: 6px; padding: 4px 6px; min-width: 0; }
  input[type="number"] { width: 90px; }
  .entries { display: grid; gap: 2px; }
  .network-view { display: grid; grid-template-columns: repeat(auto-fill, minmax(200px, 1fr)); gap: 10px; align-items: start; }
  .network-lane { border: 1px solid var(--line); border-radius: 8px; padding: 8px; display: grid; gap: 6px; background: var(--panel); }
  .network-lane h4 { display: flex; justify-content: space-between; }
  .network-card { border: 1px solid var(--line); border-radius: 6px; padding: 6px; display: flex; justify-content: space-between; gap: 6px; }
  .entry { display: flex; gap: 8px; align-items: center; padding: 4px 0; border-bottom: 1px solid var(--line); min-width: 0; }
  .grow { flex: 1 1 auto; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .pill { font-size: 11px; padding: 1px 6px; border-radius: 999px; background: var(--line); white-space: nowrap; }
  .expr { border: 1px solid var(--line); border-radius: 8px; padding: 6px; display: grid; gap: 4px; }
  .term-row { border-bottom: 0; }
  .error { color: var(--danger, #d05); }
  .icon { background: transparent; border: 0; color: var(--muted); cursor: pointer; padding: 2px 6px; }
  .json-editor { width: 100%; }
  .json-editor textarea { width: 100%; min-height: 100px; margin-top: 4px; font-family: monospace; font-size: 11px; background: var(--bg); color: var(--text); border: 1px solid var(--line); border-radius: 6px; }
  @media (max-width: 900px) { .page-body { grid-template-columns: 260px minmax(0, 1fr); } .results { display: none; } }
</style>
