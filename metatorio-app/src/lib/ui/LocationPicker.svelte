<script lang="ts">
  import { runtime } from '$lib/runtime/store.svelte';
  import type { Location } from '$lib/runtime/types';
  import Icon from './Icon.svelte';
  let {value, onChange, planetsOnly = false, label = '地点'}: {
    value: Location; onChange: (value: Location) => void; planetsOnly?: boolean; label?: string;
  } = $props();
  const planets = $derived(runtime.catalogIndex?.entries.filter(e => e.kind === 'planet') ?? []);
</script>
<label class="location-picker" title={label}>
  {#if value.kind === 'planet'}<Icon type="planet" name={value.name} size={18} />{:else}<span aria-hidden="true">✦</span>{/if}
  <select aria-label={label} value={value.kind === 'space' ? '@space' : value.name}
    onchange={event => onChange(event.currentTarget.value === '@space' ? {kind:'space'} : {kind:'planet',name:event.currentTarget.value})}>
    {#if !planetsOnly}<option value="@space">太空</option>{/if}
    {#if value.kind === 'planet' && !planets.some(p => p.name === value.name)}<option value={value.name}>{runtime.localizedName('planet',value.name)}</option>{/if}
    {#each planets as planet (planet.name)}<option value={planet.name}>{runtime.localizedName('planet',planet.name)}</option>{/each}
  </select>
</label>
<style>
  .location-picker { display:inline-flex; align-items:center; gap:3px; min-width:0; max-width:140px; flex:0 1 auto; }
  select { min-width:0; max-width:112px; padding:3px; font:inherit; color:var(--text); background:var(--panel); border:1px solid var(--line); border-radius:var(--radius-sm); }
</style>
