<script module lang="ts">
  import type { DualVar, Location, Suggestion } from "$lib/runtime/types";
  import type { MechanicScope } from "$lib/runtime/mechanic-commands";

  export type SuggestionsState = {
    flow: DualVar;
    location?: Location;
    scope: MechanicScope;
    items: Suggestion[];
    loading: boolean;
  };
</script>

<script lang="ts">
  import { runtime } from "$lib/runtime/store.svelte.ts";
  import { locationKey } from "$lib/mechanics/located";
  import type { MechanicEntry } from "$lib/runtime/types";
  import FlowRef from "$lib/ui/FlowRef.svelte";
  import HoverIcon from "$lib/ui/HoverIcon.svelte";
  import { createBackdropClose } from "$lib/ui/modal-backdrop";

  let {
    suggestions = $bindable<SuggestionsState | null>(null),
    existingMechanics = [],
  }: {
    suggestions: SuggestionsState | null;
    existingMechanics?: MechanicEntry[];
  } = $props();

  const suggestionsBackdrop = createBackdropClose(() => (suggestions = null));

  let addedKeys = $state<Set<string>>(new Set());
  let pendingKeys = $state<Set<string>>(new Set());

  $effect(() => {
    if (!suggestions || suggestions.loading) {
      addedKeys = new Set();
      pendingKeys = new Set();
    }
  });

  function candidateKey(candidate: Suggestion): string {
    return `${candidate.kind}:${candidate.name}`;
  }

  function mechanicMatchesCandidate(entry: MechanicEntry, candidate: Suggestion): boolean {
    const targetLoc = suggestions?.location;
    if (targetLoc && entry.location && locationKey(entry.location) !== locationKey(targetLoc)) {
      return false;
    }
    const m = entry.mechanic;
    switch (candidate.kind) {
      case "recipe":
        return m.type === "recipe" && m.recipe?.id === candidate.name;
      case "resource":
        return m.type === "mining" && m.resource === candidate.name;
      case "item-fuel":
        return m.type === "item-fuel" && m.item?.id === candidate.name;
      case "virtual-science":
        return m.type === "virtual-science" && m.item?.id === candidate.name;
      case "generator":
        return m.type === "generator" && m.generator?.id === candidate.name;
      case "fusion-reactor":
        return m.type === "fusion-reactor" && m.reactor?.id === candidate.name;
      case "fusion-generator":
        return m.type === "fusion-generator" && m.generator?.id === candidate.name;
      default:
        return false;
    }
  }

  function isExisting(candidate: Suggestion): boolean {
    return existingMechanics.some((entry) => mechanicMatchesCandidate(entry, candidate));
  }

  async function addCandidate(candidate: Suggestion) {
    if (!suggestions) return;
    const key = candidateKey(candidate);
    if (pendingKeys.has(key) || addedKeys.has(key)) return;
    pendingKeys = new Set([...pendingKeys, key]);
    try {
      await runtime.addSuggestion(candidate, suggestions.scope, suggestions.location);
      addedKeys = new Set([...addedKeys, key]);
    } catch {
      /* runtime.lastError 已展示 */
    } finally {
      const next = new Set(pendingKeys);
      next.delete(key);
      pendingKeys = next;
    }
  }

  function suggestionKindLabel(kind: string): string {
    return (
      {
        recipe: "配方",
        resource: "矿点",
        "item-fuel": "燃料",
        "virtual-science": "虚拟科研",
        generator: "发电机",
      }[kind] ?? kind
    );
  }

  function suggestionIcon(kind: string): { type: string; detailKind: string } {
    if (kind === "recipe") return { type: "recipe", detailKind: "recipe" };
    if (kind === "resource") return { type: "entity", detailKind: "resource" };
    if (kind === "item-fuel" || kind === "virtual-science") return { type: "item", detailKind: "item" };
    return { type: "entity", detailKind: "generator" };
  }

  function suggestionName(kind: string, name: string): string {
    const localizeKind =
      kind === "recipe" ? "recipe" : kind === "resource" ? "resource" : kind === "item-fuel" || kind === "virtual-science" ? "item" : "generator";
    return runtime.localizedName(localizeKind, name);
  }
</script>

{#if suggestions}
  <div class="backdrop low" role="button" tabindex="0" {...suggestionsBackdrop}>
    <div class="prefs-modal">
      <div class="mini-title">建议：<FlowRef flow={suggestions.flow} /></div>
      {#if suggestions.loading}
        <span class="muted">生成中…</span>
      {:else if suggestions.items.length === 0}
        <span class="muted">没有找到与该流相关的候选机制</span>
      {:else}
        {@const sections = [
          { title: "产出该流", items: suggestions.items.filter((s) => s.role !== "consumer") },
          { title: "消耗该流", items: suggestions.items.filter((s) => s.role === "consumer") },
        ]}
        {#each sections as section (section.title)}
          {#if section.items.length > 0}
            <div class="prefs-title">{section.title}</div>
            <div class="prefs-list">
              {#each section.items as candidate (candidate.kind + candidate.name)}
                {@const icon = suggestionIcon(candidate.kind)}
                {@const key = candidateKey(candidate)}
                {@const added = addedKeys.has(key)}
                {@const pending = pendingKeys.has(key)}
                {@const existed = !added && isExisting(candidate)}
                <div class="prefs-item">
                  <HoverIcon
                    type={icon.type}
                    name={candidate.name}
                    size={22}
                    detailKind={icon.detailKind}
                  />
                  <span class="prefs-name" title={candidate.name}>
                    {suggestionName(candidate.kind, candidate.name)}
                    <span class="muted">（{suggestionKindLabel(candidate.kind)}）</span>
                  </span>
                  {#if existed}
                    <span class="chip muted" title="当前工厂中已存在该机制">已存在</span>
                  {/if}
                  <button
                    class="btn"
                    class:on={added}
                    disabled={added || pending}
                    title={added ? "已添加到当前工厂" : "添加该机制"}
                    onclick={() => addCandidate(candidate)}
                  >{pending ? "添加中…" : added ? "✓ 已添加" : "添加"}</button>
                </div>
              {/each}
            </div>
          {/if}
        {/each}
      {/if}
      <div class="mini-actions">
        <button class="btn primary" onclick={() => (suggestions = null)}>关闭</button>
      </div>
    </div>
  </div>
{/if}

<style>
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

  .backdrop.low {
    z-index: 30;
  }

  .prefs-modal {
    width: min(460px, 100%);
    max-height: min(720px, calc(100vh - 48px));
    display: grid;
    gap: 14px;
    padding: 18px;
    overflow-y: auto;
    background: var(--panel);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius);
    box-shadow: var(--shadow-strong);
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

  .mini-actions {
    display: flex;
    justify-content: flex-end;
    gap: 6px;
    padding-top: 12px;
    margin-top: 2px;
    border-top: 1px solid var(--line);
  }

  .prefs-title {
    margin: 6px 0 2px;
    color: var(--faint);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.1em;
    text-transform: uppercase;
  }

  .prefs-list {
    display: grid;
    gap: 2px;
    min-width: 0;
  }

  .prefs-item {
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 3px 4px;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    transition: background var(--transition-fast), border-color var(--transition-fast);
  }

  .prefs-item:hover {
    background: var(--card-hover);
    border-color: var(--line);
  }

  .prefs-name {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    font-size: 11.5px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
