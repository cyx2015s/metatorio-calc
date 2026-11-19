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
  import FlowRef from "$lib/ui/FlowRef.svelte";
  import HoverIcon from "$lib/ui/HoverIcon.svelte";
  import { backdropKeydown } from "$lib/ui/WorkspaceDialogs.svelte";

  let {
    suggestions = $bindable<SuggestionsState | null>(null),
  }: {
    suggestions: SuggestionsState | null;
  } = $props();

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
  <div class="backdrop low" role="button" tabindex="0" onclick={(e) => { if (e.target === e.currentTarget) suggestions = null; }} onkeydown={(e) => backdropKeydown(e, () => (suggestions = null))}>
    <div class="prefs-modal">
      <div class="mini-title">建议：<FlowRef flow={suggestions.flow} /></div>
      {#if suggestions.loading}
        <span class="muted">生成中…</span>
      {:else if suggestions.items.length === 0}
        <span class="muted">没有找到与该流相关的候选机制</span>
      {:else}
        {@const producers = suggestions.items.filter((s) => s.role !== "consumer")}
        {@const consumers = suggestions.items.filter((s) => s.role === "consumer")}
        {#if producers.length > 0}
          <div class="prefs-title">产出该流</div>
          <div class="prefs-list">
            {#each producers as candidate (candidate.kind + candidate.name)}
              {@const icon = suggestionIcon(candidate.kind)}
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
                <button
                  class="btn"
                  title="添加该机制"
                  onclick={() =>
                    runtime
                      .addSuggestion(candidate, suggestions?.scope, suggestions?.location)
                      .catch(() => {})
                      .then(() => (suggestions = null))}
                >添加</button>
              </div>
            {/each}
          </div>
        {/if}
        {#if consumers.length > 0}
          <div class="prefs-title">消耗该流</div>
          <div class="prefs-list">
            {#each consumers as candidate (candidate.kind + candidate.name)}
              {@const icon = suggestionIcon(candidate.kind)}
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
                <button
                  class="btn"
                  title="添加该机制"
                  onclick={() =>
                    runtime
                      .addSuggestion(candidate, suggestions?.scope, suggestions?.location)
                      .catch(() => {})
                      .then(() => (suggestions = null))}
                >添加</button>
              </div>
            {/each}
          </div>
        {/if}
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
