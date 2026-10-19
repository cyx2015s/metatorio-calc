<script lang="ts">
  import { dndzone } from 'svelte-dnd-action';
  import { runtime } from '$lib/runtime/store.svelte';
  import { mechanicPrototype } from '$lib/display-labels';
  import type { MechanicEntry } from '$lib/runtime/types';
  import type { MechanicViewProps } from '$lib/mechanics/workspace';
  import MechanicCard from './MechanicCard.svelte';
  import MechanicRow from './MechanicRow.svelte';

  let {project, factory, entries, visibleIds, mergeRecipes, actions, solutions}: {
    mergeRecipes: boolean;
  } & MechanicViewProps = $props();
  let filterActive = $derived(visibleIds !== null);
  let dragMechanics = $state<MechanicEntry[]>([]);
  $effect(() => { dragMechanics = entries; });
  let visibleMechanics = $derived.by(() => {
    if (visibleIds === null) return dragMechanics;
    const ids = new Set(visibleIds);
    return dragMechanics.filter(entry => ids.has(entry.id));
  });
  function groupTitle(entries: MechanicEntry[]): string {
    const first = entries[0]?.mechanic;
    if (!first) return '';
    const ref = mechanicPrototype(first);
    return runtime.localizedName(ref.kind, ref.name);
  }
  function groupMachine(entries: MechanicEntry[]): string {
    const machine = entries[0]?.mechanic.machine?.id;
    return machine ? runtime.localizedName('entity', machine) || machine : '';
  }
  /** 聚合键：合并开启时仅 recipe 按 (配方, 机器) 聚合；其余类型各自独立。 */
  function mechGroupKey(entry: import("$lib/runtime/types").MechanicEntry): string {
    if (!mergeRecipes) return `single:${entry.id}`;
    const m = entry.mechanic;
    if (m.type === "recipe") return `recipe:${m.recipe?.id ?? ""}:${m.machine?.id ?? ""}`;
    // 非 recipe 机制：永不合并（每机制独立一组）。
    return `single:${entry.id}`;
  }

  /** 网格分组：保持原始顺序，同键条目聚成一组（不同品质放一行）。 */
  let mechGroups = $derived(
    (() => {
      const groups: {
        key: string;
        entries: import("$lib/runtime/types").MechanicEntry[];
      }[] = [];
      const index = new Map<string, number>();
      for (const entry of visibleMechanics) {
        const key = mechGroupKey(entry);
        const existing = index.get(key);
        if (existing !== undefined) {
          groups[existing].entries.push(entry);
        } else {
          index.set(key, groups.length);
          groups.push({ key, entries: [entry] });
        }
      }
      return groups;
    })(),
  );
  type MechanicGroup = { id: string; key: string; entries: import("$lib/runtime/types").MechanicEntry[] };
  let dragGroups = $state<MechanicGroup[]>([]);
  $effect(() => { dragGroups = mechGroups.map((group) => ({ ...group, id: group.key })); });
  function handleMechanicsConsider(event: CustomEvent<{ items: MechanicGroup[] }>) {
    if (filterActive) return;
    dragGroups = event.detail.items;
  }
  function handleMechanicsFinalize(event: CustomEvent<{ items: MechanicGroup[] }>) {
    if (filterActive) return;
    dragGroups = event.detail.items;
    dragMechanics = event.detail.items.flatMap((group) => group.entries);
    actions.commands.reorderMechanics(dragMechanics.map((entry) => entry.id)).catch(() => {});
  }
</script>

{#snippet row(entry: MechanicEntry)}
  <MechanicRow mechanicId={entry.id} virtualize={entries.length > 80}>
    {#snippet children(pin)}
      <MechanicCard {entry} {project} {factory} {actions}
        solution={solutions.get(entry.id) ?? null} onInteractionChange={pin} />
    {/snippet}
  </MechanicRow>
{/snippet}
      <div
        class="mech-list"
        use:dndzone={{ items: dragGroups, type: "mechanic-groups", flipDurationMs: 120, dragDisabled: filterActive }}
        onconsider={handleMechanicsConsider}
        onfinalize={handleMechanicsFinalize}
      >
        {#each dragGroups as group (group.id)}
          {#if group.entries.length > 1}
            <!-- 配方合并组：标题（配方名 + 机器名），子机制完整显示 -->
            <div class="mech-group">
              <div class="mech-group-head">
                <span class="mech-group-title" title={groupTitle(group.entries)}>
                  {groupTitle(group.entries)}
                </span>
                {#if groupMachine(group.entries)}
                  <span class="chip">{groupMachine(group.entries)}</span>
                {/if}
                <span class="chip muted">{group.entries.length} 变体</span>
              </div>
              {#each group.entries as entry (entry.id)}
                <div class="mech-group-row">
                  {@render row(entry)}
                </div>
              {/each}
            </div>
          {:else}
            {#each group.entries as entry (entry.id)}
              {@render row(entry)}
            {/each}
          {/if}
        {/each}
      </div>
<style>
  .mech-list {
    /* 关键：机制列表必须按内容高度撑开，由 .center 负责滚动。
       若它作为可收缩 flex 项被压到剩余高度，内部的 .mech-group 会因
       overflow:hidden（自动最小尺寸为 0）被压成一条 2px 的横线。 */
    flex: 0 0 auto;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding-top: 6px;
  }

  .mech-group {
    flex: 0 0 auto;
    overflow: hidden;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: var(--radius);
  }

  .mech-group-head {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 7px 10px;
    background: var(--panel-elev);
    border-bottom: 1px solid var(--line);
  }

  .mech-group-title {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    color: var(--muted);
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-overflow: ellipsis;
    text-transform: uppercase;
    white-space: nowrap;
  }

  .mech-group-row + .mech-group-row {
    border-top: 1px solid var(--line);
  }

  /* 分组内的机制卡去掉自己的外框，由分组容器统一承载边界。 */
  .mech-group-row :global(.mech-card) {
    border: 0;
    border-radius: 0;
    background: transparent;
  }

  .mech-group-row :global(.mech-card:hover) {
    background: color-mix(in srgb, var(--panel) 82%, var(--text) 4%);
  }

  /* 顶层机制卡由 MechanicCard 自己提供边框；这里只保证悬停/间距一致。 */
  .mech-list > :global(.mechanic-row) {
    flex: 0 0 auto;
    margin-bottom: 0;
  }

</style>
