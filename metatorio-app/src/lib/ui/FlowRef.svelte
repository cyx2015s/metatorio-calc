<script lang="ts">
  import { runtime } from "$lib/runtime/store.svelte.ts";
  import { presentFlow, type FlowMatch } from "$lib/dual-var";
  import { flowName } from "$lib/display-labels";
  import type { DualVar } from "$lib/runtime/types";
  import HoverIcon from "./HoverIcon.svelte";
  import RichText from "./RichText.svelte";
  let { flow, match, size = 22, showName = true, showQualifiers = true, showBadge = true, grow = false, nested = false,
    title, onClick, onContextMenu, flowRate, rateBasis }: {
    flow: DualVar; match?: FlowMatch; size?: number; showName?: boolean; showQualifiers?: boolean; showBadge?: boolean; grow?: boolean; nested?: boolean;
    title?: string; onClick?: (event: MouseEvent) => void; onContextMenu?: (event: MouseEvent) => void;
    flowRate?: number; rateBasis?: "solution" | "unit";
  } = $props();
  const info = $derived(presentFlow(flow, (kind, name) => runtime.localizedName(kind, name), match));
  const richLabel = $derived(flowName(flow, (kind, name) => runtime.richName(kind, name)));
  const tip = $derived(title ? `${info.description} · ${title}` : info.description);
</script>

<span class="flow-ref" class:grow data-flow-kind={info.kind} data-wildcard={info.wildcard || undefined} title={tip}>
  <span class="flow-symbol">
    <HoverIcon type={info.type} name={info.name} detailKind={info.detailKind} quality={info.quality} {size}
      {flow} {match} {flowRate} {rateBasis} title={tip} {onClick} {onContextMenu} focusable={!nested} />
    {#if info.wildcard && info.kind !== "Fluid"}<span class="wildcard" aria-label="任意品质">*</span>{/if}
  </span>
  {#if showBadge && info.badge}<span class="semantic-badge">{info.badge}</span>{/if}
  {#if showName}<span class="flow-name">{#if info.kind === "Custom"}{info.label}{:else}<RichText text={richLabel} />{/if}</span>{/if}
  {#if showName && showQualifiers}
    <span class="qualifiers">{info.qualifiers.filter(value => !showBadge || value !== info.badge).join(" · ")}</span>
  {/if}
</span>

<style>
  .flow-ref { display: inline-flex; align-items: center; gap: 4px; min-width: 0; max-width: 100%; vertical-align: middle; }
  .grow { flex: 1; }
  .flow-symbol { position: relative; display: inline-flex; flex: 0 0 auto; }
  .flow-name { min-width: 0; overflow: hidden; text-overflow: ellipsis; }
  .qualifiers { color: var(--muted); font-size: 10px; overflow-wrap: anywhere; }
  .semantic-badge { font-size: 9px; line-height: 1.2; color: var(--muted); border: 1px solid var(--line); border-radius: 3px; padding: 1px 2px; flex: 0 0 auto; }
  .wildcard { position: absolute; bottom: -2px; left: -2px; font: bold 11px var(--mono); padding: 0 2px; background: var(--panel); border: 1px solid var(--line); border-radius: 3px; pointer-events: none; }
</style>
