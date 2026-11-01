<script lang="ts">
  import { runtime } from "$lib/runtime/store.svelte.ts";
  import type { ProjectDocument, ProjectAction, OrbitalTransportCosts } from "$lib/runtime/types";
  import Selector from "./Selector.svelte";
  import RichText from "./RichText.svelte";
  import { diagnosticFrom } from "$lib/diagnostic";
  import { DEFAULT_ORBITAL_TRANSPORT_COSTS } from "$lib/runtime/project-settings";

  let { project }: { project: ProjectDocument } = $props();
  let pending = $state(false);
  let error = $state("");
  let picking = $state(false);
  let transportCosts = $derived({ ...DEFAULT_ORBITAL_TRANSPORT_COSTS, ...project.settings.orbital_transport_costs });
  const costs: { key: keyof OrbitalTransportCosts; label: string; unit: string }[] = [
    { key: "rocket_export_per_kg", label: "火箭出口成本", unit: "每 kg 物品" },
    { key: "orbital_drop_per_stack", label: "轨道投送成本", unit: "每组物品" },
    { key: "orbital_drop_spoil_penalty", label: "变质额外成本系数", unit: "除以有效变质时间（tick）" },
  ];

  async function save(action: ProjectAction) {
    if (pending) return;
    const id = project.id;
    pending = true;
    error = "";
    try {
      await runtime.send({ scope: "project", action: { project: id, action } });
    } catch (cause) {
      error = diagnosticFrom(cause).message;
    } finally {
      pending = false;
    }
  }

  function number(event: Event, previous: number | null, maximum = Infinity): number | null {
    const input = event.currentTarget as HTMLInputElement;
    const raw = input.value.trim();
    const value = Number(raw);
    if (!raw || !Number.isFinite(value) || value < 0 || value > maximum || (maximum === 255 && !Number.isInteger(value))) {
      error = maximum === 255 ? "邻接数量必须是 0–255 的整数。" : "成本必须是有限的非负数。";
      input.value = previous === null ? "" : String(previous);
      return null;
    }
    return value;
  }

  function setNeighbours(name: string, value: number | null) {
    const estimates = { ...project.settings.reactor_neighbours };
    if (value === null) delete estimates[name];
    else estimates[name] = value;
    return save({ "set-reactor-neighbours": { estimates } });
  }
</script>

<section class="panel" aria-label="项目估算设置">
  <div class="section-head"><span>反应堆与轨道运输</span><span class="count">项目级估算</span></div>
  <fieldset disabled={pending}>
    <legend>反应堆邻接数量</legend>
    <p class="muted">按原型设置，裂变与聚变共用；未覆盖时为 3。单个机制显式指定的邻接数量优先。</p>
    {#each Object.entries(project.settings.reactor_neighbours ?? {}) as [name, value] (name)}
      <div class="estimate-row">
        <span><RichText text={runtime.richName("entity", name)} /></span>
        <input type="number" min="0" max="255" step="1" value={value}
          aria-label={`邻接数量：${name}`}
          onchange={async (event) => {
            const input = event.currentTarget;
            const next = number(event, value, 255);
            if (next !== null) {
              await setNeighbours(name, next);
              input.value = String(project.settings.reactor_neighbours[name] ?? 3);
            }
          }} />
        <button class="icon-btn danger" aria-label={`恢复默认邻接数量：${name}`}
          title="移除覆盖，恢复默认值 3" onclick={() => setNeighbours(name, null)}>×</button>
      </div>
    {:else}
      <p class="muted">所有反应堆使用默认估算。</p>
    {/each}
    <button class="btn block" disabled={!runtime.activeContext} onclick={() => (picking = true)}>添加反应堆估算…</button>
    {#if Object.keys(project.settings.reactor_neighbours ?? {}).length}
      <button class="btn block" onclick={() => save({ "set-reactor-neighbours": { estimates: {} } })}>恢复全部默认邻接数量</button>
    {/if}
  </fieldset>
  <fieldset disabled={pending}>
    <legend>轨道运输成本</legend>
    <p class="muted">影响宇宙工厂的方案选优；不改变物料、火箭载荷或运输量。与显示时间刻度无关。</p>
    {#each costs as { key, label, unit } (key)}
      <label class="field">
        <span>{label}</span>
        <input type="number" min="0" step="any" value={transportCosts[key]}
          aria-label={label} onchange={async (event) => {
            const input = event.currentTarget;
            const next = number(event, transportCosts[key]);
            if (next !== null) {
              await save({ "set-orbital-transport-costs": { costs: { ...transportCosts, [key]: next } } });
              input.value = String(transportCosts[key] ?? "");
            }
          }} />
        <span class="muted">{unit}</span>
      </label>
    {/each}
  </fieldset>
  {#if pending}<p role="status">正在保存…</p>{/if}
  {#if error}<p role="alert">{error}</p>{/if}
</section>

{#if picking}
  <Selector kind="reactor" title="选择反应堆" respectAccessibility={false}
    kindOptions={[{ kind: "reactor", label: "裂变反应堆" }, { kind: "fusion-reactor", label: "聚变反应堆" }]}
    onSelect={(name) => {
      picking = false;
      if (!(name in (project.settings.reactor_neighbours ?? {}))) void setNeighbours(name, 3);
    }} onClose={() => (picking = false)} />
{/if}

<style>
  .panel { padding: 14px 16px; background: transparent; border: 0; border-bottom: 1px solid var(--line); border-radius: 0; box-shadow: none; }
  .section-head { display: flex; justify-content: space-between; gap: 8px; font-weight: 600; }
  .count { color: var(--muted); font-size: 10px; }
  fieldset { border: 0; padding: 0; margin: 0; min-width: 0; }
  legend { font-weight: 600; margin-top: 12px; }
  .estimate-row { display: flex; gap: 6px; align-items: center; margin: 6px 0; }
  .estimate-row > span { flex: 1; min-width: 0; overflow-wrap: anywhere; }
  input { min-width: 0; width: 100%; box-sizing: border-box; }
  .estimate-row input { width: 70px; }
  .field, .btn.block { margin-top: 8px; }
  [role="alert"] { color: var(--danger, #e66); overflow-wrap: anywhere; }
</style>
