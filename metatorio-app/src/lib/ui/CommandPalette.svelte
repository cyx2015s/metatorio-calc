<script module lang="ts">
  export interface PaletteCommand {
    id: string;
    group: string;
    label: string;
    detail?: string;
    shortcut?: string;
    keywords?: string;
    badge?: string;
    disabled?: boolean;
    action: () => void | Promise<void>;
  }
</script>

<script lang="ts">
  import { tick } from "svelte";

  let {
    open = $bindable(false),
    commands,
  }: {
    open: boolean;
    commands: PaletteCommand[];
  } = $props();

  let query = $state("");
  let activeIndex = $state(0);
  let inputEl = $state<HTMLInputElement | null>(null);
  let listEl = $state<HTMLDivElement | null>(null);
  let startedOnBackdrop = false;
  let previousActive: HTMLElement | null = null;

  const filtered = $derived.by(() => {
    const terms = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
    const list =
      terms.length === 0
        ? commands
        : commands.filter((cmd) => {
            const haystack = `${cmd.group} ${cmd.label} ${cmd.detail ?? ""} ${cmd.keywords ?? ""} ${cmd.badge ?? ""}`.toLowerCase();
            return terms.every((term) => haystack.includes(term));
          });
    return list.slice(0, 80);
  });

  $effect(() => {
    if (open) {
      previousActive =
        typeof document !== "undefined" && document.activeElement instanceof HTMLElement
          ? document.activeElement
          : null;
      query = "";
      activeIndex = 0;
      void tick().then(() => {
        inputEl?.focus();
        inputEl?.select();
      });
    }
  });

  $effect(() => {
    const len = filtered.length;
    if (len === 0) {
      activeIndex = 0;
    } else if (activeIndex >= len) {
      activeIndex = len - 1;
    }
  });

  function scrollActiveIntoView() {
    void tick().then(() => {
      listEl?.querySelector<HTMLElement>('[data-active="true"]')?.scrollIntoView({ block: "nearest" });
    });
  }

  function close(restoreFocus = true) {
    open = false;
    startedOnBackdrop = false;
    if (restoreFocus && previousActive?.isConnected) {
      const target = previousActive;
      previousActive = null;
      void tick().then(() => target.focus());
    } else {
      previousActive = null;
    }
  }

  function execute(cmd: PaletteCommand | undefined) {
    if (!cmd || cmd.disabled) return;
    close(false);
    Promise.resolve(cmd.action()).catch(() => {});
  }

  function moveActive(delta: number) {
    if (filtered.length === 0) return;
    let next = (activeIndex + delta + filtered.length) % filtered.length;
    // Skip disabled items when possible
    for (let step = 0; step < filtered.length; step += 1) {
      if (!filtered[next]?.disabled) break;
      next = (next + (delta >= 0 ? 1 : -1) + filtered.length) % filtered.length;
    }
    activeIndex = next;
    scrollActiveIntoView();
  }

  function handlePaletteKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      close(true);
      return;
    }
    if (event.key === "ArrowDown") {
      event.preventDefault();
      moveActive(1);
      return;
    }
    if (event.key === "ArrowUp") {
      event.preventDefault();
      moveActive(-1);
      return;
    }
    if (event.key === "Home") {
      event.preventDefault();
      activeIndex = 0;
      scrollActiveIntoView();
      return;
    }
    if (event.key === "End" && filtered.length > 0) {
      event.preventDefault();
      activeIndex = filtered.length - 1;
      scrollActiveIntoView();
      return;
    }
    if (event.key === "Enter") {
      event.preventDefault();
      execute(filtered[activeIndex]);
    }
  }

  function handleBackdropPointerDown(event: PointerEvent) {
    startedOnBackdrop = event.target === event.currentTarget && event.button === 0;
  }

  function handleBackdropClick(event: MouseEvent) {
    const shouldClose = startedOnBackdrop && event.target === event.currentTarget;
    startedOnBackdrop = false;
    if (shouldClose) close(true);
  }

  function handleBackdropKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      close(true);
    } else if (
      (event.key === "Enter" || event.key === " ") &&
      event.target === event.currentTarget
    ) {
      event.preventDefault();
      close(true);
    }
  }
</script>

{#if open}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    class="backdrop"
    role="presentation"
    onpointerdown={handleBackdropPointerDown}
    onclick={handleBackdropClick}
    onkeydown={handleBackdropKeydown}
  >
    <div
      class="palette"
      role="dialog"
      aria-modal="true"
      aria-label="命令面板"
      tabindex="-1"
      onkeydown={handlePaletteKeydown}
    >
      <div class="palette-head">
        <span class="palette-prompt" aria-hidden="true">›</span>
        <input
          bind:this={inputEl}
          bind:value={query}
          type="text"
          class="palette-input"
          aria-label="搜索命令、工厂、星球或机制"
          placeholder="搜索命令、工厂、星球、目标或机制…"
          oninput={() => {
            activeIndex = 0;
          }}
        />
        <kbd class="palette-esc">Esc</kbd>
      </div>

      <div class="palette-list" bind:this={listEl} role="listbox" aria-label="命令列表">
        {#each filtered as cmd, index (cmd.id)}
          {@const showGroup = index === 0 || filtered[index - 1].group !== cmd.group}
          {#if showGroup}
            <div class="palette-group">{cmd.group}</div>
          {/if}
          <button
            type="button"
            role="option"
            class="palette-item"
            class:active={index === activeIndex}
            data-active={index === activeIndex}
            aria-selected={index === activeIndex}
            disabled={cmd.disabled}
            onmouseenter={() => {
              if (!cmd.disabled) activeIndex = index;
            }}
            onclick={() => execute(cmd)}
          >
            <span class="palette-item-main">
              <span class="palette-item-label">{cmd.label}</span>
              {#if cmd.detail}
                <span class="palette-item-detail">{cmd.detail}</span>
              {/if}
            </span>
            <span class="palette-item-meta">
              {#if cmd.badge}
                <span class="chip">{cmd.badge}</span>
              {/if}
              {#if cmd.shortcut}
                <kbd>{cmd.shortcut}</kbd>
              {/if}
            </span>
          </button>
        {:else}
          <div class="palette-empty">没有匹配的命令或对象</div>
        {/each}
      </div>

      <div class="palette-foot">
        <span>↑/↓ 选择</span>
        <span>Enter 执行</span>
        <span>Esc 关闭</span>
      </div>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    z-index: 60;
    inset: 0;
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding: min(12vh, 88px) 16px 24px;
    background: var(--backdrop);
    backdrop-filter: blur(6px);
  }

  .palette {
    width: min(580px, 100%);
    max-height: min(540px, 76vh);
    display: flex;
    flex-direction: column;
    overflow: hidden;
    color: var(--text);
    background: var(--panel);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius);
    box-shadow: var(--shadow-strong);
  }

  .palette-head {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    background: var(--panel-elev);
    border-bottom: 1px solid var(--line);
  }

  .palette-prompt {
    color: var(--faint);
    font-family: var(--mono);
    font-size: 16px;
    line-height: 1;
    user-select: none;
  }

  .palette-input {
    flex: 1 1 auto;
    min-width: 0;
    padding: 4px 0;
    color: var(--text);
    background: transparent;
    border: 0;
    font-size: 13px;
    outline: none;
  }

  .palette-input::placeholder {
    color: var(--faint);
  }

  .palette-esc,
  .palette-item-meta kbd {
    padding: 2px 6px;
    color: var(--muted);
    background: var(--input-bg);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    font-family: var(--mono);
    font-size: 10px;
    line-height: 1.2;
    white-space: nowrap;
  }

  .palette-list {
    flex: 1 1 auto;
    min-height: 0;
    overflow-y: auto;
    padding: 6px;
  }

  .palette-group {
    padding: 8px 9px 4px;
    color: var(--faint);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.1em;
    text-transform: uppercase;
  }

  .palette-group:first-child {
    padding-top: 4px;
  }

  .palette-item {
    width: 100%;
    min-height: 34px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 6px 10px;
    color: var(--text);
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    text-align: left;
    cursor: pointer;
    transition: background var(--transition-fast), border-color var(--transition-fast);
  }

  .palette-item:hover:not(:disabled) {
    background: var(--card-hover);
  }

  .palette-item.active {
    background: var(--accent-dim);
    border-color: var(--line-strong);
    box-shadow: inset 2px 0 0 var(--accent);
  }

  .palette-item:disabled {
    opacity: 0.42;
    cursor: not-allowed;
  }

  .palette-item-main {
    min-width: 0;
    display: flex;
    align-items: baseline;
    gap: 8px;
    flex: 1 1 auto;
  }

  .palette-item-label {
    overflow: hidden;
    font-size: 12px;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .palette-item-detail {
    overflow: hidden;
    color: var(--muted);
    font-size: 11px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .palette-item-meta {
    flex: 0 0 auto;
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }

  .palette-empty {
    padding: 28px 16px;
    color: var(--faint);
    font-size: 12px;
    text-align: center;
  }

  .palette-foot {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 7px 14px;
    color: var(--faint);
    background: var(--panel-elev);
    border-top: 1px solid var(--line);
    font-family: var(--mono);
    font-size: 10px;
  }
</style>
