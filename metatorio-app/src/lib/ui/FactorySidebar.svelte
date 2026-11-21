<script lang="ts">
  import { onMount, tick, type Snippet } from 'svelte';
  let {tab=$bindable('factory'),factoryName,projectName,factoryContent,projectContent}: {
    tab:'factory'|'project';factoryName:string;projectName:string;factoryContent:Snippet;projectContent:Snippet;
  }=$props();

  const scrollPositions: Record<'factory' | 'project', number> = { factory: 0, project: 0 };
  let scrollEl = $state<HTMLDivElement | null>(null);
  let activeTab: 'factory' | 'project' = tab;
  let restoring = false;
  let prevHeight = 0;

  function handleScroll() {
    if (restoring || !scrollEl || scrollEl.clientHeight === 0) return;
    scrollPositions[activeTab] = scrollEl.scrollTop;
  }

  $effect.pre(() => {
    const nextTab = tab;
    if (nextTab === activeTab) return;
    if (scrollEl && scrollEl.clientHeight > 0) {
      scrollPositions[activeTab] = scrollEl.scrollTop;
    }
    activeTab = nextTab;
    restoring = true;
    const saved = scrollPositions[nextTab] ?? 0;
    void tick().then(() => {
      if (scrollEl) scrollEl.scrollTop = saved;
      requestAnimationFrame(() => {
        if (scrollEl) scrollEl.scrollTop = saved;
        restoring = false;
      });
    });
  });

  onMount(() => {
    if (!scrollEl) return;
    prevHeight = scrollEl.clientHeight;
    const observer = new ResizeObserver(() => {
      if (!scrollEl) return;
      const height = scrollEl.clientHeight;
      if (prevHeight === 0 && height > 0) {
        const saved = scrollPositions[activeTab] ?? 0;
        restoring = true;
        scrollEl.scrollTop = saved;
        requestAnimationFrame(() => {
          restoring = false;
        });
      }
      prevHeight = height;
    });
    observer.observe(scrollEl);
    return () => observer.disconnect();
  });
</script>
<div class="rail-tabs" role="tablist" aria-label="左栏编辑范围">
  {#each [{id:'factory' as const,label:'工厂',name:factoryName},{id:'project' as const,label:'项目',name:projectName}] as item}
    <button class="rail-tab" class:active={tab===item.id} role="tab" aria-selected={tab===item.id} onclick={()=>tab=item.id}>
      <span class="rail-tab-main">{item.label}</span><span class="rail-tab-sub">{item.name}</span>
    </button>
  {/each}
</div>
<div class="rail-scroll" bind:this={scrollEl} onscroll={handleScroll}>
  {#if tab==='factory'}{@render factoryContent()}{:else}{@render projectContent()}{/if}
</div>
<style>
  .rail-tabs {
    flex: 0 0 auto;
    display: flex;
    position: sticky;
    top: 0;
    z-index: 2;
    background: var(--surface-glass);
    backdrop-filter: blur(8px);
    border-bottom: 1px solid var(--line);
  }
  .rail-scroll {
    flex: 1 1 auto;
    min-height: 0;
    overflow-y: auto;
    overflow-x: hidden;
  }
  @media (max-width: 920px) {
    .rail-scroll {
      overflow-y: visible;
    }
  }
  .rail-tab {
    flex: 1;
    min-width: 0;
    display: grid;
    gap: 3px;
    text-align: left;
    padding: 11px 16px;
    border: 0;
    border-bottom: 2px solid transparent;
    background: none;
    color: var(--muted);
    font: inherit;
    cursor: pointer;
    transition:
      color var(--transition-fast),
      background var(--transition-fast),
      border-color var(--transition-fast);
  }
  .rail-tab:hover {
    color: var(--text);
    background: var(--card-hover);
  }
  .rail-tab.active {
    color: var(--text);
    border-bottom-color: var(--accent);
    background: color-mix(in srgb, var(--accent-dim) 55%, transparent);
  }
  .rail-tab-main {
    font-size: 12px;
    font-weight: 650;
    letter-spacing: 0.02em;
  }
  .rail-tab-sub {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 10px;
    color: var(--faint);
  }
  .rail-tab.active .rail-tab-sub {
    color: var(--muted);
  }
</style>
