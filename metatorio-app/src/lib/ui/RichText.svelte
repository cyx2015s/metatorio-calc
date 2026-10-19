<script lang="ts">
  import { parseRichText, type RichToken } from "$lib/rich-text";
  import { runtime } from "$lib/runtime/store.svelte.ts";
  import Icon from "./Icon.svelte";
  let { text }: { text: string } = $props();
  const tokens = $derived(parseRichText(text));
  function name(kind: string, id: string): string { return runtime.localizedName(kind, id); }
  function referenceLabel(token: Extract<RichToken, {type: "icon"}>): string {
    return name(token.kind, token.name) + (token.quality && token.quality !== "normal" ? ` (${name("quality", token.quality)})` : "");
  }
</script>

<span class="rich-text">
  {#each tokens as token}
    {#if token.type === "text"}
      <span style:color={token.color} style:font-weight={token.bold ? "700" : undefined} style:font-family={token.mono ? "monospace" : undefined}>{token.text}</span>
    {:else}
      <span class="rich-reference" role="img" style:color={token.color} style:font-weight={token.bold ? "700" : undefined} title={referenceLabel(token)} aria-label={referenceLabel(token)}>
        <Icon type={token.kind === "planet" ? "planet" : token.kind} name={token.name} size={16} title={name(token.kind, token.name)} fallbackLabel={name(token.kind, token.name)} />
        {#if token.quality && token.quality !== "normal"}<span class="quality"><Icon type="quality" name={token.quality} size={9} /></span>{/if}
      </span>
    {/if}
  {/each}
</span>

<style>
  .rich-text { white-space: pre-wrap; overflow-wrap: anywhere; }
  .rich-reference { display: inline-flex; position: relative; vertical-align: -0.15em; white-space: nowrap; max-width: 100%; min-width: 0; }
  .quality { position: absolute; left: -2px; bottom: -2px; }
</style>
