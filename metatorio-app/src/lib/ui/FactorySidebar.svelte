<script lang="ts">
  import type { Snippet } from 'svelte';
  let {tab=$bindable('factory'),factoryName,projectName,factoryContent,projectContent}: {
    tab:'factory'|'project';factoryName:string;projectName:string;factoryContent:Snippet;projectContent:Snippet;
  }=$props();
</script>
<div class="rail-tabs" role="tablist" aria-label="左栏编辑范围">
  {#each [{id:'factory' as const,label:'工厂',name:factoryName},{id:'project' as const,label:'项目',name:projectName}] as item}
    <button class="rail-tab" class:active={tab===item.id} role="tab" aria-selected={tab===item.id} onclick={()=>tab=item.id}>
      <span class="rail-tab-main">{item.label}</span><span class="rail-tab-sub">{item.name}</span>
    </button>
  {/each}
</div>
{#if tab==='factory'}{@render factoryContent()}{:else}{@render projectContent()}{/if}
<style>
  .rail-tabs {display:flex;position:sticky;top:0;z-index:2;background:var(--panel);border-bottom:1px solid var(--line)}
  .rail-tab {flex:1;min-width:0;display:grid;gap:3px;text-align:left;padding:12px 16px;border:0;border-bottom:2px solid transparent;background:none;color:var(--muted);font:inherit;cursor:pointer}
  .rail-tab.active {color:var(--text);border-bottom-color:var(--text)}
  .rail-tab-main {font-size:12px;font-weight:600}.rail-tab-sub {overflow:hidden;text-overflow:ellipsis;white-space:nowrap;font-size:10px;color:var(--muted)}
</style>
