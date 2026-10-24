<script lang="ts">
  import type { DualVar } from '$lib/runtime/types';
  let {flow, onChange}: {flow: DualVar; onChange: (flow: DualVar) => void} = $props();
  const fluid = $derived(typeof flow === 'object' && 'Fluid' in flow ? flow.Fluid as {name:string;temperature:[number,number]} : null);
  function change(raw: string, side: number) {
    const value = Number(raw.trim()); if (!fluid || !raw.trim() || !Number.isFinite(value)) return;
    const range = [...fluid.temperature]; range[side] = Math.round(value); range.sort((a,b) => a-b);
    onChange({Fluid: {name: fluid.name, temperature: range as [number,number]}});
  }
</script>
{#if fluid}<span class="temp-range">
  <input aria-label="期望温度下限（℃）" value={fluid.temperature[0]} onchange={e => change(e.currentTarget.value,0)} inputmode="numeric" />
  <span>~</span><input aria-label="期望温度上限（℃）" value={fluid.temperature[1]} onchange={e => change(e.currentTarget.value,1)} inputmode="numeric" /><span>℃</span>
</span>{/if}
<style>
  .temp-range {display:inline-flex;align-items:center;gap:2px;font-size:10px;color:var(--muted)}
  input {width:40px;min-width:0;padding:3px;border:1px solid var(--line);background:var(--panel);color:var(--text);border-radius:var(--radius-sm);font:inherit}
</style>
