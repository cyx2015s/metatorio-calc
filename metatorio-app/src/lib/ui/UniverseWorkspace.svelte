<script lang="ts">
  import { runtime } from "$lib/runtime/store.svelte";
  import type { IdWithQuality, Location, MechanicKind, UniverseDocument } from "$lib/runtime/types";

  let { universe }: { universe: UniverseDocument } = $props();
  let locationMode = $state<"planet" | "space">("planet");
  let planet = $state("nauvis");
  let transferMode = $state<"planet" | "space">("planet");
  let transferPlanet = $state("nauvis");
  let mechanicKind = $state<MechanicKind>("recipe");
  let transferItem = $state("iron-plate");
  let transferQuality = $state("normal");
  let targetItem = $state("iron-plate");
  let targetQuality = $state("normal");
  let targetAmount = $state(1);
  let targetMode = $state<"planet" | "space">("planet");
  let targetPlanet = $state("nauvis");
  let solving = $state(false);
  async function solve() {
    if (solving) return;
    solving = true;
    try {
      await runtime.solveSelectedUniverse();
    } finally {
      solving = false;
    }
  }

  const location = (): Location =>
    locationMode === "space" ? { kind: "space" } : { kind: "planet", name: planet.trim() || "nauvis" };
  const targetLocation = (): Location =>
    targetMode === "space" ? { kind: "space" } : { kind: "planet", name: targetPlanet.trim() || "nauvis" };
  const item = (id: string, quality: string): IdWithQuality => ({ id: id.trim(), quality: quality.trim() || "normal" });
  const projectQuality = () => runtime.selectedProject?.settings.major_quality || "normal";
  const presentFlow = (flow: unknown) => {
    if (typeof flow === "string") return flow;
    if (flow && typeof flow === "object" && "Item" in flow) return (flow as { Item: IdWithQuality }).Item.id;
    if (flow && typeof flow === "object" && "Fluid" in flow) return (flow as { Fluid: { name: string } }).Fluid.name;
    return JSON.stringify(flow);
  };

  let drafts = $state<Record<number, { a: string; b: string }>>({});
  const draft = (id: number) => (drafts[id] ??= { a: "", b: "" });
  const applyRecipe = async (id: number) => {
    const d = draft(id);
    await runtime.updateUniverseMechanic(id, { recipe: { "set-recipe": { recipe: item(d.a, projectQuality()) } } });
    if (d.b.trim()) {
      await runtime.updateUniverseMechanic(id, { recipe: { "set-machine": { machine: item(d.b, "normal") } } });
    }
  };
  const applyMining = async (id: number) => {
    const d = draft(id);
    if (d.a.trim()) await runtime.updateUniverseMechanic(id, { mining: { "set-resource": { resource: d.a.trim() } } });
    if (d.b.trim()) await runtime.updateUniverseMechanic(id, { mining: { "set-machine": { machine: item(d.b, "normal") } } });
  };
  const applyItemFuel = (id: number) => {
    const d = draft(id);
    if (d.a.trim()) runtime.updateUniverseMechanic(id, { "item-fuel": { "set-item": { item: item(d.a, projectQuality()) } } }).catch(() => {});
  };

  const kinds: MechanicKind[] = [
    "recipe", "mining", "spoil", "plant", "item-fuel", "item-launch", "generator", "boiler",
    "reactor", "solar", "fluid-fuel", "fluid-heat", "tile-extract", "tile-dispose", "virtual-science",
  ];
</script>

<div class="universe-backdrop">
  <div class="universe-panel">
    <header>
      <div>
        <strong>宇宙工厂</strong>
        <span>{universe.name} · {universe.mechanics.length} 个机制 · {universe.targets.length} 个目标</span>
      </div>
      <div class="actions">
        <button class="btn" disabled={solving} onclick={solve}>{solving ? "求解中…" : "求解"}</button>
        <button class="btn ghost" onclick={() => runtime.removeUniverse(universe.id).catch(() => {})}>删除</button>
        <button class="btn ghost" onclick={() => runtime.selectUniverse(null)}>关闭</button>
      </div>
    </header>

    <div class="body">
      <section>
        <h3>约束</h3>
        <div class="row">
          <label><input type="checkbox" checked={universe.strict_source} onchange={() => runtime.setUniverseStrictSource(!universe.strict_source).catch(() => {})} /> 严格供给</label>
          <label><input type="checkbox" checked={universe.strict_sink} onchange={() => runtime.setUniverseStrictSink(!universe.strict_sink).catch(() => {})} /> 严格消耗</label>
        </div>
      </section>

      <section>
        <h3>添加机制</h3>
        <div class="row">
          <select bind:value={locationMode}>
            <option value="planet">星球</option>
            <option value="space">太空</option>
          </select>
          {#if locationMode === "planet"}<input bind:value={planet} placeholder="星球名" />{/if}
          <select bind:value={mechanicKind}>
            {#each kinds as kind}<option value={kind}>{kind}</option>{/each}
          </select>
          <button class="btn" onclick={() => runtime.addUniverseLocal(location(), mechanicKind).catch(() => {})}>添加本地机制</button>
        </div>
        <div class="row">
          <select bind:value={transferMode}>
            <option value="planet">星球</option>
            <option value="space">太空</option>
          </select>
          {#if transferMode === "planet"}<input bind:value={transferPlanet} placeholder="星球名" />{/if}
          <input bind:value={transferItem} placeholder="物品 id" />
          <input bind:value={transferQuality} placeholder="品质" />
          <button class="btn" onclick={() => runtime.addUniverseTransfer(transferMode === "space" ? { kind: "space" } : { kind: "planet", name: transferPlanet.trim() || "nauvis" }, item(transferItem, transferQuality)).catch(() => {})}>添加火箭转移</button>
        </div>
      </section>

      <section>
        <h3>机制</h3>
        {#each universe.mechanics as entry (entry.id)}
          <div class="entry">
            <span class="pill">{entry.mechanic.location.kind === "space" ? "太空" : entry.mechanic.location.name}</span>
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
            <button class="btn ghost" onclick={() => runtime.removeUniverseMechanic(entry.id).catch(() => {})}>删除</button>
          </div>
        {/each}
      </section>

      <section>
        <h3>目标</h3>
        <div class="row">
          <select bind:value={targetMode}>
            <option value="planet">星球</option>
            <option value="space">太空</option>
          </select>
          {#if targetMode === "planet"}<input bind:value={targetPlanet} placeholder="星球名" />{/if}
          <input bind:value={targetItem} placeholder="物品 id" />
          <input bind:value={targetQuality} placeholder="品质" />
          <input type="number" bind:value={targetAmount} />
          <button class="btn" onclick={() => runtime.addUniverseTarget({ location: targetLocation(), flow: { Item: item(targetItem, targetQuality) } }, targetAmount).catch(() => {})}>添加目标</button>
        </div>
        {#each universe.targets as target (target.id)}
          <div class="entry">
            <span class="pill">{target.flow.location.kind === "space" ? "太空" : target.flow.location.name}</span>
            <span>{presentFlow(target.flow.flow)} ×</span>
            <input type="number" value={target.amount} onchange={(event) => runtime.setUniverseTargetAmount(target.id, Number(event.currentTarget.value)).catch(() => {})} />
            <button class="btn ghost" onclick={() => runtime.removeUniverseTarget(target.id).catch(() => {})}>删除</button>
          </div>
        {/each}
      </section>

      <section>
        <h3>结果</h3>
        {#if runtime.universeSolveError}<p class="error">{runtime.universeSolveError}</p>{/if}
        {#if runtime.universeSolve}
          {#if runtime.universeSolve.status.status === "solved"}
            <p>成本 {runtime.universeSolve.status.cost}</p>
            <h4>机制用量</h4>
            {#each runtime.universeSolve.status.mechanics as solution (solution.mechanic)}
              <div class="entry"><span class="pill">{solution.location.kind === "space" ? "太空" : solution.location.name}</span><span>#{solution.mechanic}: {solution.amount}</span></div>
            {/each}
            <h4>流量</h4>
            {#each runtime.universeSolve.status.flows as flow (JSON.stringify(flow.flow))}
              <div class="entry"><span class="pill">{flow.flow.location.kind === "space" ? "太空" : flow.flow.location.name}</span><span>{presentFlow(flow.flow.flow)}: {flow.amount}</span></div>
            {/each}
          {:else}
            <p class="error">{runtime.universeSolve.status.description}</p>
          {/if}
        {/if}
      </section>
    </div>
  </div>
</div>

<style>
  .universe-backdrop { position: fixed; inset: 0; z-index: 50; background: rgba(0,0,0,.45); display: grid; place-items: center; }
  .universe-panel { width: min(1100px, 94vw); height: min(800px, 90vh); display: flex; flex-direction: column; background: var(--panel); border: 1px solid var(--line-strong); border-radius: 12px; overflow: hidden; }
  header { display: flex; justify-content: space-between; gap: 12px; padding: 12px 16px; border-bottom: 1px solid var(--line); }
  header div { display: flex; flex-direction: column; gap: 2px; }
  header span { color: var(--muted); font-size: 12px; }
  .actions { flex-direction: row !important; align-items: center; }
  .body { overflow: auto; padding: 14px 16px; display: grid; gap: 18px; }
  section { display: grid; gap: 8px; }
  h3, h4 { margin: 0; font-size: 13px; }
  .row { display: flex; gap: 6px; flex-wrap: wrap; align-items: center; }
  input, select { background: var(--bg); color: var(--text); border: 1px solid var(--line); border-radius: 6px; padding: 4px 6px; }
  input[type="number"] { width: 90px; }
  .entry { display: flex; gap: 8px; align-items: center; padding: 4px 0; border-bottom: 1px solid var(--line); }
  .pill { font-size: 11px; padding: 1px 6px; border-radius: 999px; background: var(--line); }
  .error { color: var(--danger, #d05); }
  .json-editor { width: 100%; }
  .json-editor textarea { width: 100%; min-height: 120px; margin-top: 4px; font-family: monospace; font-size: 11px; background: var(--bg); color: var(--text); border: 1px solid var(--line); border-radius: 6px; }
</style>
