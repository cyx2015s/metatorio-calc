<script module lang="ts">
  export type ConfirmState = {
    message: string;
    action: () => void;
    confirmLabel?: string;
    danger?: boolean;
  };

  export type RenameContextState = {
    id: string;
    name: string;
  };

  export type RenameTargetState = {
    kind: "project" | "factory";
    name: string;
  };

  export type CloseProjectState = {
    project: number;
    name: string;
  };

  /** 遮罩/可点击背景的键盘访达：Enter/Space/Esc 与点击一致（a11y）。 */
  export function backdropKeydown(event: KeyboardEvent, close: () => void): void {
    if (event.key === "Enter" || event.key === " " || event.key === "Escape") {
      event.preventDefault();
      close();
    }
  }
</script>

<script lang="ts">
  import { runtime } from "$lib/runtime/store.svelte.ts";

  let {
    newProjectOpen = $bindable(false),
    newFactoryOpen = $bindable(false),
    newUniverseOpen = $bindable(false),
    confirmState = $bindable<ConfirmState | null>(null),
    renameCtx = $bindable<RenameContextState | null>(null),
    renameTarget = $bindable<RenameTargetState | null>(null),
    closeProjectState = $bindable<CloseProjectState | null>(null),
  }: {
    newProjectOpen: boolean;
    newFactoryOpen: boolean;
    newUniverseOpen: boolean;
    confirmState: ConfirmState | null;
    renameCtx: RenameContextState | null;
    renameTarget: RenameTargetState | null;
    closeProjectState: CloseProjectState | null;
  } = $props();

  let newProjectName = $state("新项目");
  let newFactoryName = $state("新工厂");
  let newUniverseName = $state("新宇宙工厂");
  let renameCtxName = $state("");
  let renameTargetName = $state("");

  $effect(() => {
    if (renameCtx) renameCtxName = renameCtx.name;
  });

  $effect(() => {
    if (renameTarget) renameTargetName = renameTarget.name;
  });

  async function createProject() {
    const name = newProjectName.trim();
    if (!name) return;
    newProjectOpen = false;
    try {
      await runtime.newProject(name);
    } catch {
      /* lastError 已展示 */
    }
  }

  async function createUniverse() {
    const name = newUniverseName.trim();
    if (!name) return;
    newUniverseOpen = false;
    try {
      await runtime.addUniverse(name);
    } catch {
      // runtime store 已直接暴露本次命令错误，此处无需重复弹窗。
    }
  }

  async function createFactory() {
    const name = newFactoryName.trim();
    if (!name) return;
    newFactoryOpen = false;
    try {
      await runtime.addFactory(name);
    } catch {
      /* lastError 已展示 */
    }
  }

  function confirmRename() {
    const name = renameCtxName.trim();
    if (renameCtx && name) {
      runtime.renameContext(renameCtx.id, name).catch(() => {});
    }
    renameCtx = null;
  }

  function confirmRenameTarget() {
    const name = renameTargetName.trim();
    if (!renameTarget || !name) {
      renameTarget = null;
      return;
    }
    if (renameTarget.kind === "project") {
      runtime.setProjectName(name).catch(() => {});
    } else {
      runtime.setFactoryName(name).catch(() => {});
    }
    renameTarget = null;
  }
</script>

{#if newProjectOpen}
  <div class="backdrop" role="button" tabindex="0" onclick={(e) => { if (e.target === e.currentTarget) newProjectOpen = false; }} onkeydown={(e) => backdropKeydown(e, () => (newProjectOpen = false))}>
    <div class="mini-modal">
      <div class="mini-title">新建项目</div>
      <input
        bind:value={newProjectName}
        onkeydown={(event) => {
          if (event.key === "Enter") createProject();
        }}
      />
      <div class="mini-actions">
        <button class="btn ghost" onclick={() => (newProjectOpen = false)}>取消</button>
        <button class="btn primary" onclick={createProject}>创建</button>
      </div>
    </div>
  </div>
{/if}

{#if newUniverseOpen}
  <div class="backdrop" role="button" tabindex="0" onclick={(e) => { if (e.target === e.currentTarget) newUniverseOpen = false; }} onkeydown={(e) => backdropKeydown(e, () => (newUniverseOpen = false))}>
    <div class="mini-modal">
      <div class="mini-title">新建宇宙工厂</div>
      <input
        bind:value={newUniverseName}
        onkeydown={(event) => {
          if (event.key === "Enter") createUniverse();
        }}
      />
      <div class="mini-actions">
        <button class="btn ghost" onclick={() => (newUniverseOpen = false)}>取消</button>
        <button class="btn primary" onclick={createUniverse}>创建</button>
      </div>
    </div>
  </div>
{/if}

{#if newFactoryOpen}
  <div class="backdrop" role="button" tabindex="0" onclick={(e) => { if (e.target === e.currentTarget) newFactoryOpen = false; }} onkeydown={(e) => backdropKeydown(e, () => (newFactoryOpen = false))}>
    <div class="mini-modal">
      <div class="mini-title">新建工厂</div>
      <input
        bind:value={newFactoryName}
        onkeydown={(event) => {
          if (event.key === "Enter") createFactory();
        }}
      />
      <div class="mini-actions">
        <button class="btn ghost" onclick={() => (newFactoryOpen = false)}>取消</button>
        <button class="btn primary" onclick={createFactory}>创建</button>
      </div>
    </div>
  </div>
{/if}

{#if confirmState}
  {@const confirm = confirmState}
  <div class="backdrop" role="button" tabindex="0" onclick={(e) => { if (e.target === e.currentTarget) confirmState = null; }} onkeydown={(e) => backdropKeydown(e, () => (confirmState = null))}>
    <div class="mini-modal">
      <div class="mini-title">确认</div>
      <div class="confirm-text">{confirm.message}</div>
      <div class="mini-actions">
        <button class="btn ghost" onclick={() => (confirmState = null)}>取消</button>
        <button
          class="btn {confirm.danger ? 'danger' : 'primary'}"
          onclick={() => {
            confirm.action();
            confirmState = null;
          }}
        >{confirm.confirmLabel ?? "确定"}</button>
      </div>
    </div>
  </div>
{/if}

{#if renameCtx}
  <div class="backdrop" role="button" tabindex="0" onclick={(e) => { if (e.target === e.currentTarget) renameCtx = null; }} onkeydown={(e) => backdropKeydown(e, () => (renameCtx = null))}>
    <div class="mini-modal">
      <div class="mini-title">重命名上下文</div>
      <input
        bind:value={renameCtxName}
        onkeydown={(event) => {
          if (event.key === "Enter") confirmRename();
        }}
      />
      <div class="mini-actions">
        <button class="btn ghost" onclick={() => (renameCtx = null)}>取消</button>
        <button class="btn primary" onclick={confirmRename}>确定</button>
      </div>
    </div>
  </div>
{/if}

{#if renameTarget}
  <div class="backdrop" role="button" tabindex="0" onclick={(e) => { if (e.target === e.currentTarget) renameTarget = null; }} onkeydown={(e) => backdropKeydown(e, () => (renameTarget = null))}>
    <div class="mini-modal">
      <div class="mini-title">{renameTarget.kind === "project" ? "重命名项目" : "重命名工厂"}</div>
      <input
        bind:value={renameTargetName}
        onkeydown={(event) => {
          if (event.key === "Enter") confirmRenameTarget();
        }}
      />
      <div class="mini-actions">
        <button class="btn ghost" onclick={() => (renameTarget = null)}>取消</button>
        <button class="btn primary" onclick={confirmRenameTarget}>确定</button>
      </div>
    </div>
  </div>
{/if}

{#if closeProjectState}
  {@const closing = closeProjectState}
  <div class="backdrop" role="button" tabindex="0" onclick={(e) => { if (e.target === e.currentTarget) closeProjectState = null; }} onkeydown={(e) => backdropKeydown(e, () => (closeProjectState = null))}>
    <div class="mini-modal">
      <div class="mini-title">关闭项目「{closing.name}」</div>
      <div class="confirm-text">
        关闭后项目从当前工作区移除（文件不会删除）。未保存的改动将丢失。
      </div>
      <div class="mini-actions">
        <button class="btn ghost" onclick={() => (closeProjectState = null)}>取消</button>
        <button
          class="btn"
          onclick={() => {
            runtime.closeProject("save").catch(() => {});
            closeProjectState = null;
          }}
        >保存并关闭</button>
        <button
          class="btn danger"
          onclick={() => {
            runtime.closeProject("discard").catch(() => {});
            closeProjectState = null;
          }}
        >不保存关闭</button>
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

  .mini-modal {
    width: min(352px, 100%);
    display: grid;
    gap: 14px;
    padding: 18px;
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

  .confirm-text {
    color: var(--muted);
    font-size: 11.5px;
    line-height: 1.55;
  }

  .mini-modal input {
    min-height: 32px;
    padding: 0 9px;
    color: var(--text);
    background: var(--input-bg);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-sm);
    font-size: 12px;
    transition: border-color var(--transition-fast), box-shadow var(--transition-fast);
  }

  .mini-modal input:focus {
    border-color: var(--accent);
    outline: none;
    box-shadow: 0 0 0 1px var(--accent);
  }

  .mini-actions {
    display: flex;
    justify-content: flex-end;
    gap: 6px;
    padding-top: 12px;
    margin-top: 2px;
    border-top: 1px solid var(--line);
  }
</style>
