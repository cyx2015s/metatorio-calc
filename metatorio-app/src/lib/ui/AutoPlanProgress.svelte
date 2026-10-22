<script lang="ts">
  import type { AutoPlanProgress } from "$lib/runtime/types";
  let { progress }: { progress: AutoPlanProgress | null } = $props();
  const labels = {
    preparing: "准备规划",
    enumerating: "枚举候选",
    building: "构建模型",
    solving: "求解中",
    verifying: "验证规划",
  };
</script>

<span class="auto-plan-progress" role="status" aria-live="polite" aria-atomic="true">
  {labels[progress?.stage ?? "preparing"]}
  {#if progress?.variables != null && progress?.constraints != null}
    · {progress.variables.toLocaleString("zh-CN")} 个变量
    · {progress.constraints.toLocaleString("zh-CN")} 条约束
  {:else}
    · 变量数、约束数待构建
  {/if}
</span>

<style>
  .auto-plan-progress {
    color: var(--muted);
    font-size: 0.82rem;
    font-variant-numeric: tabular-nums;
    overflow-wrap: anywhere;
  }
</style>
