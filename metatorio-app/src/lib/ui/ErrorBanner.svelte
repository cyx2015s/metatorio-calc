<script lang="ts">
  import { runtime } from "$lib/runtime/store.svelte.ts";
  import type { DualVar, Location } from "$lib/runtime/types";
  import DiagnosticView from "$lib/ui/DiagnosticView.svelte";
  import { diagnosticFrom, diagnosticText } from "$lib/diagnostic";

  let {
    notice,
    noticeTone = "info",
    missingProjectContext,
    onOpenData,
    onSuggest,
    onNotice,
    onDismissNotice,
  }: {
    notice: string | null;
    noticeTone?: "info" | "warn" | "danger";
    missingProjectContext: boolean;
    onOpenData: () => void;
    onSuggest: (flow: DualVar, location?: Location) => void;
    onNotice: (message: string) => void;
    onDismissNotice?: () => void;
  } = $props();

  // 顶端报错条：最多 3 行，过长省略；被截断时给一个复制按钮把完整内容取回来。
  // 求解器的报错会把缺供给的物品全列出来，原文能把顶部撑满、挤掉下面的内容。
  let errLines = $state<HTMLElement | null>(null);
  let errTruncated = $state(false);
  let dismissed = $state(false);

  $effect(() => {
    void notice;
    dismissed = false;
  });

  function errorStripText(): string {
    return [
      runtime.contextError ? `数据：${runtime.contextError}` : "",
      runtime.lastError
        ? `操作：${diagnosticText(
            runtime.lastDiagnostic?.message === runtime.lastError
              ? runtime.lastDiagnostic
              : diagnosticFrom(runtime.lastError),
            (kind, name) => runtime.localizedName(kind, name),
          )}`
        : "",
    ]
      .filter(Boolean)
      .join("\n");
  }

  async function copyErrorStrip() {
    try {
      await navigator.clipboard.writeText(errorStripText());
      onNotice("已复制完整报错");
    } catch {
      onNotice("复制失败：剪贴板不可用");
    }
  }

  // 量「有没有被截断」：纵向溢出（超过 3 行）或任一行横向溢出（省略号生效）。
  // 持久型数据源变化或窗口宽度变化时重新量。
  $effect(() => {
    const node = errLines;
    void runtime.contextError;
    void runtime.lastError;
    if (!node) {
      errTruncated = false;
      return;
    }
    const measure = () => {
      const vertical = node.scrollHeight > node.clientHeight + 1;
      const horizontal = Array.from(node.children).some(
        (child) => (child as HTMLElement).scrollWidth > (child as HTMLElement).clientWidth + 1,
      );
      errTruncated = vertical || horizontal;
    };
    const frame = requestAnimationFrame(measure);
    window.addEventListener("resize", measure);
    return () => {
      cancelAnimationFrame(frame);
      window.removeEventListener("resize", measure);
    };
  });
</script>

{#if runtime.openingProjectPath || runtime.fileOpenNotice || missingProjectContext}
  <div class="file-feedback" role="status" aria-live="polite">
    {#if runtime.openingProjectPath}<span title={runtime.openingProjectPath}>正在打开工程：{runtime.openingProjectPath}</span>
    {:else if runtime.fileOpenNotice}<span>{runtime.fileOpenNotice}</span><button class="icon-btn" aria-label="关闭文件提示" onclick={() => (runtime.fileOpenNotice = null)}>×</button>{/if}
    {#if missingProjectContext}
      <span>工程已打开，但引用的游戏数据不在本机缓存中。请选择或加载对应的数据。</span>
      <button class="btn" onclick={onOpenData}>选择游戏数据</button>
    {/if}
  </div>
{/if}

{#if runtime.contextError || runtime.lastError}
  <div class="err-strip">
    <div
      class="err-lines"
      class:structured={runtime.lastDiagnostic &&
        runtime.lastDiagnostic.flows.length +
          runtime.lastDiagnostic.no_provider.length +
          runtime.lastDiagnostic.no_consumer.length >
          0}
      bind:this={errLines}
    >
      {#if runtime.contextError}
        <span class="err-line" title={runtime.contextError}>数据：{runtime.contextError}</span>
      {/if}
      {#if runtime.lastError}
        <div>
          <strong>操作失败</strong><DiagnosticView
            diagnostic={runtime.lastDiagnostic?.message === runtime.lastError
              ? runtime.lastDiagnostic
              : diagnosticFrom(runtime.lastError)}
            onFlowClick={onSuggest}
          />
        </div>
      {/if}
    </div>
    {#if errTruncated}
      <button class="err-copy" title="复制完整报错" onclick={copyErrorStrip}>复制</button>
    {/if}
  </div>
{/if}

{#if notice && !dismissed}
  <div
    class="toast-notice"
    class:tone-warn={noticeTone === "warn"}
    class:tone-danger={noticeTone === "danger"}
    class:tone-info={noticeTone === "info"}
    role="status"
    aria-live="polite"
  >
    <span class="toast-dot" aria-hidden="true"></span>
    <span class="toast-text">{notice}</span>
    <button
      class="icon-btn toast-close"
      type="button"
      title="关闭提示"
      aria-label="关闭提示"
      onclick={() => {
        dismissed = true;
        onDismissNotice?.();
      }}
    >×</button>
  </div>
{/if}

<style>
  .file-feedback {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 4px 8px;
    padding: 4px 16px;
    border-bottom: 1px solid var(--line);
    color: var(--muted);
    font-size: 11px;
  }

  .file-feedback > span {
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .err-strip {
    flex: 0 0 auto;
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 6px 14px;
    color: var(--danger);
    background: var(--danger-dim);
    border-bottom: 1px solid var(--danger-line);
    font-size: 11px;
  }

  .err-lines {
    flex: 1 1 auto;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-height: calc(3 * 1.45em);
    overflow: hidden;
  }

  .err-line {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .err-lines.structured {
    max-height: min(260px, 35vh);
    overflow: auto;
  }

  .err-copy {
    flex: 0 0 auto;
    height: 22px;
    padding: 0 8px;
    color: inherit;
    background: transparent;
    border: 1px solid var(--danger-line);
    border-radius: var(--radius-sm);
    font-size: 10px;
    cursor: pointer;
    transition: background var(--transition-fast), border-color var(--transition-fast);
  }

  .err-copy:hover {
    background: color-mix(in srgb, var(--danger) 14%, transparent);
  }

  .toast-notice {
    position: fixed;
    right: 16px;
    bottom: 16px;
    z-index: 70;
    max-width: min(380px, calc(100vw - 32px));
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 8px 7px 12px;
    color: var(--text);
    background: var(--surface-glass, var(--panel));
    backdrop-filter: blur(12px);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius);
    box-shadow: var(--shadow-strong);
    font-size: 11.5px;
    line-height: 1.4;
  }

  .toast-dot {
    width: 6px;
    height: 6px;
    flex: 0 0 auto;
    border-radius: 50%;
    background: var(--accent);
  }

  .toast-notice.tone-warn {
    color: var(--warn);
    background: var(--warn-dim);
    border-color: var(--warn-line);
  }

  .toast-notice.tone-warn .toast-dot {
    background: var(--warn);
  }

  .toast-notice.tone-danger {
    color: var(--danger);
    background: var(--danger-dim);
    border-color: var(--danger-line);
  }

  .toast-notice.tone-danger .toast-dot {
    background: var(--danger);
  }

  .toast-text {
    flex: 1 1 auto;
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .toast-close {
    flex: 0 0 auto;
    color: inherit;
  }
</style>
