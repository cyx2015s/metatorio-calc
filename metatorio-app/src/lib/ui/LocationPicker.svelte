<script lang="ts">
  import { runtime } from '$lib/runtime/store.svelte';
  import type { Location } from '$lib/runtime/types';
  import { locationKey, locationLabel } from '$lib/mechanics/located';
  import LocationIcon from './LocationIcon.svelte';
  import SecondaryMenu from './SecondaryMenu.svelte';
  let {value, onChange, planetsOnly=false, label='地点'}: {
    value:Location;onChange:(value:Location)=>void;planetsOnly?:boolean;label?:string;
  }=$props();
  const choices=$derived.by(()=>{
    const planets:Location[]=(runtime.catalogIndex?.entries.filter(e=>e.kind==='planet')??[]).map(e=>({kind:'planet',name:e.name}));
    if(value.kind==='planet'&&!planets.some(p=>locationKey(p)===locationKey(value)))planets.unshift(value);
    return planetsOnly?planets:[...planets,{kind:'space'} as Location];
  });
  const name=(location:Location)=>locationLabel(location,(k,n)=>runtime.localizedName(k,n));
</script>
<SecondaryMenu title={label}>
  {#snippet trigger(toggle,open)}
    <button class="location-picker" aria-label={label} title={`${label}：${name(value)}`} aria-haspopup="dialog" aria-expanded={open} onclick={toggle}><LocationIcon location={value} /></button>
  {/snippet}
  {#snippet children(close)}
    <div class="location-grid">
      {#each choices as location (locationKey(location))}
        <button class="location-choice" class:selected={locationKey(location)===locationKey(value)} aria-label={name(location)} aria-pressed={locationKey(location)===locationKey(value)} title={name(location)} onclick={()=>{onChange(location);close();}}><LocationIcon {location} size={36} /></button>
      {/each}
    </div>
  {/snippet}
</SecondaryMenu>
<style>
  .location-picker,
  .location-choice {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: 0 0 auto;
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    background: var(--panel-elev);
    color: var(--text);
    cursor: pointer;
    transition:
      background var(--transition-fast),
      border-color var(--transition-fast),
      box-shadow var(--transition-fast);
  }
  .location-picker {
    min-width: 26px;
    min-height: 26px;
    padding: 2px;
  }
  .location-grid {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .location-choice {
    min-width: 44px;
    min-height: 44px;
    padding: 6px;
  }
  .location-picker:hover,
  .location-choice:hover {
    background: var(--card-hover);
    border-color: var(--line-strong);
  }
  .location-picker:focus-visible,
  .location-choice:focus-visible {
    outline: 1.5px solid var(--focus);
    outline-offset: 1px;
  }
  .selected,
  .selected:hover {
    border-color: var(--accent);
    background: var(--accent-dim);
    box-shadow: inset 0 0 0 1px var(--accent);
  }
</style>
