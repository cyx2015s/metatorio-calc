<script lang="ts">
  import type { SolveDiagnostics } from "$lib/runtime/types";
  import { recentPivots, optimizationLabel, resultOptimization } from "$lib/optimization";

  let { report }: { report?: SolveDiagnostics } = $props();
  const optimization = $derived(resultOptimization(report));
  const pivots = $derived(optimization ? recentPivots(optimization) : []);
  function cost(value: number): string {
    return value.toLocaleString("zh-CN", { maximumSignificantDigits: 10 });
  }
  function count(value: number | null | undefined): string {
    return value == null ? "未记录计数" : value.toLocaleString("zh-CN");
  }
</script>

{#if optimization}
  <div class="optimization-summary" role="status">
    <strong>{optimizationLabel(optimization)}</strong>
    <span class="timing">{report?.planning_optimization ? "自动规划" : "求解"}耗时 {optimization.elapsed_seconds.toFixed(1)} 秒 · 时限 {optimization.time_limit_seconds.toFixed(1)} 秒</span>
    <span class="timing">浮点/可行性搜索换基 {count(optimization.floating_search_pivots)}；精确优化换基 {count(optimization.optimization_pivots)}，其中成本下降 {count(optimization.improving_pivots)}</span>
    <span class="timing">精确优化阶段耗时 {optimization.optimization_elapsed_seconds == null ? "未记录" : `${optimization.optimization_elapsed_seconds.toFixed(1)} 秒`}{#if optimization.first_feasible_seconds != null} · 首次已验证可行解耗时 {optimization.first_feasible_seconds.toFixed(1)} 秒{/if}</span>
    {#if optimization.optimization_pivots === 0 && (optimization.optimization_elapsed_seconds ?? 0) > 0}
      <span class="timing">精确优化阶段已耗时，但没有已完成的精确优化换基记录。</span>
    {/if}
    {#if pivots.length > 0}
      <details>
        <summary>最近 {pivots.length} 次精确优化换基的成本变化</summary>
        <ul>
          {#each pivots as pivot}
            <li>第 {pivot.pivot} 次换基：{cost(pivot.cost_before)} → {cost(pivot.cost_after)}，下降 {cost(pivot.decrease)}</li>
          {/each}
        </ul>
      </details>
    {:else}
      <span class="timing">未记录到精确优化换基的成本变化（不包含浮点搜索和可行性修复）。</span>
    {/if}
  </div>
{/if}

<style>
  .optimization-summary {
    display: grid;
    gap: 0.4rem;
    margin: 0.75rem 0;
    padding: 0.75rem;
    border: 1px solid var(--line);
    border-radius: 8px;
    font-size: 0.82rem;
    overflow-wrap: anywhere;
  }
  .timing { color: var(--muted); }
  details { font-variant-numeric: tabular-nums; }
  summary { cursor: pointer; }
  ul { padding-left: 1.2rem; margin: 0.5rem 0 0; }
  li + li { margin-top: 0.3rem; }
</style>
