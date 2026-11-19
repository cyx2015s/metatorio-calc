<script lang="ts">
  import type { SolveDiagnostics } from "$lib/runtime/types";
  import { recentPivots, optimizationLabel, resultOptimization } from "$lib/optimization";

  let { report }: { report?: SolveDiagnostics } = $props();
  const optimization = $derived(resultOptimization(report));
  const pivots = $derived(optimization ? recentPivots(optimization) : []);
  function cost(value: number | null): string {
    return value == null ? "—" : value.toLocaleString("zh-CN", { maximumSignificantDigits: 10 });
  }
  function count(value: number | null | undefined): string {
    return value == null ? "未记录计数" : value.toLocaleString("zh-CN");
  }
</script>

{#if optimization}
  <div class="optimization-summary" role="status">
    <strong>{optimizationLabel(optimization)}</strong>
    <span class="timing">{report?.planning_optimization ? "自动规划" : "求解"}耗时 {optimization.elapsed_seconds?.toFixed(1) ?? "—"} 秒 · 时限 {optimization.time_limit_seconds?.toFixed(1) ?? "—"} 秒</span>
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
    gap: 5px;
    margin: 0 0 14px;
    padding: 10px 12px;
    background: var(--panel-elev);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    font-size: 11px;
    line-height: 1.45;
    overflow-wrap: anywhere;
  }
  .optimization-summary strong {
    color: var(--text);
    font-size: 11.5px;
    font-weight: 650;
  }
  .timing {
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }
  details {
    margin-top: 2px;
    padding-top: 6px;
    border-top: 1px solid var(--line);
    font-variant-numeric: tabular-nums;
  }
  summary {
    color: var(--text);
    font-size: 10.5px;
    font-weight: 600;
    cursor: pointer;
  }
  summary:hover {
    color: var(--accent);
  }
  ul {
    padding-left: 16px;
    margin: 6px 0 0;
    color: var(--muted);
    font-family: var(--mono);
    font-size: 10.5px;
  }
  li + li { margin-top: 4px; }
</style>
