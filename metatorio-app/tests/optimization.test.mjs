import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { compile } from 'svelte/compiler';
import { render } from 'svelte/server';
import { recentPivots, optimizationLabel, resultOptimization } from '../src/lib/optimization.ts';

const source = await readFile(new URL('../src/lib/ui/OptimizationSummary.svelte', import.meta.url), 'utf8');
const compiled = compile(source.replace('$lib/optimization', new URL('../src/lib/optimization.ts', import.meta.url).href), {
  generate: 'server', filename: 'OptimizationSummary.svelte',
}).js.code.replace(/from (['"])(svelte(?:\/[^'"]*)?)\1/g, (_, quote, name) => `from ${quote}${import.meta.resolve(name)}${quote}`);
const { default: OptimizationSummary } = await import(`data:text/javascript;base64,${Buffer.from(compiled).toString('base64')}`);
const html = report => render(OptimizationSummary, { props: { report } }).body;

const optimum = {
  optimality_proven: true, termination_reason: 'optimal', elapsed_seconds: 2,
  time_limit_seconds: 30, recent_pivot_cost_changes: [],
};
const timeout = { ...optimum, optimality_proven: false, termination_reason: 'time-limit' };

test('full planning timeout takes precedence over an optimal writeback subset', () => {
  const chosen = resultOptimization({ planning_optimization: timeout, optimization: optimum, exact_optimality_verified: true });
  assert.equal(chosen, timeout);
  assert.equal(optimizationLabel(chosen), '已到时限，返回已验证可行方案（未确认最优）');
});

test('ordinary solves expose optimality and other search stops without inference from certificates', () => {
  assert.equal(optimizationLabel(resultOptimization({ optimization: optimum })), '已确认最优');
  assert.equal(optimizationLabel({ ...timeout, termination_reason: 'search-stopped' }), '未确认最优');
  assert.equal(resultOptimization(), null);
  assert.equal(resultOptimization({ exact_optimality_verified: true }), null);
  assert.equal(resultOptimization({ planning_optimization: null, optimization: null }), null);
});

test('progress retains degenerate pivots and exact decreases despite rounded cost ties', () => {
  const real = { pivot: 42, cost_before: 10, cost_after: 8, decrease: 2 };
  const degenerate = { pivot: 43, cost_before: 8, cost_after: 8, decrease: 0 };
  const roundedTie = { pivot: 44, cost_before: 8, cost_after: 8, decrease: Number.EPSILON };
  assert.deepEqual(recentPivots({ ...timeout, recent_pivot_cost_changes: [
    real,
    degenerate,
    roundedTie,
    { pivot: 45, cost_before: 8, cost_after: 9, decrease: -1 },
    { pivot: 45, cost_before: Infinity, cost_after: 8, decrease: Infinity },
  ] }), [real, degenerate, roundedTie]);
  assert.deepEqual(recentPivots(timeout), []);
});

test('rendered timeout uses the full model and displays real cost progress', () => {
  const content = html({ optimization: optimum, planning_optimization: {
    ...timeout, recent_pivot_cost_changes: [
      { pivot: 42, cost_before: 10, cost_after: 8, decrease: 2 },
      { pivot: 43, cost_before: 8, cost_after: 8, decrease: 0 },
      { pivot: 44, cost_before: 8, cost_after: 8, decrease: Number.EPSILON },
    ],
  } });
  assert.match(content, /已到时限，返回已验证可行方案（未确认最优）/);
  assert.match(content, /自动规划耗时/);
  assert.match(content, /第 42 次换基：10 → 8，下降 2/);
  assert.match(content, /第 43 次换基：8 → 8，下降 0/);
  assert.match(content, /第 44 次换基：8 → 8，下降 0\.0000000000000002220446049/);
  assert.doesNotMatch(content, /已确认最优/);
});

test('rendered empty history separates exact optimization history from search pivots', () => {
  const content = html({ optimization: timeout });
  assert.match(content, /未记录到精确优化换基的成本变化（不包含浮点搜索和可行性修复）/);
  assert.doesNotMatch(content, /最近 \d+ 次精确优化换基/);
  assert.match(html({ optimization: optimum }), /已确认最优/);
  assert.match(html({ optimization: { ...timeout, termination_reason: 'search-stopped' } }), /未确认最优/);
});

test('legacy serialized results render no new optimality claim', () => {
  for (const report of [undefined, {}, { exact_optimality_verified: true }]) {
    assert.doesNotMatch(html(report), /确认最优|时限|成本变化|优化换基/);
  }
});

test('floating search optimum does not imply exact optimization pivots occurred', () => {
  const content = html({ optimization: {
    ...optimum, floating_search_pivots: 1023, optimization_pivots: 0,
    improving_pivots: 0, optimization_elapsed_seconds: 0, first_feasible_seconds: 1.2,
  } });
  assert.match(content, /已确认最优/);
  assert.match(content, /浮点\/可行性搜索换基 1,023；精确优化换基 0，其中成本下降 0/);
  assert.match(content, /精确优化阶段耗时 0\.0 秒/);
  assert.match(content, /首次已验证可行解耗时 1\.2 秒/);
  assert.match(content, /不包含浮点搜索和可行性修复/);
});

test('time spent before completing a certified pivot is explicit', () => {
  const content = html({ planning_optimization: {
    ...timeout, floating_search_pivots: 50, optimization_pivots: 0,
    improving_pivots: 0, optimization_elapsed_seconds: 14.6, first_feasible_seconds: 2.3,
  } });
  assert.match(content, /精确优化阶段耗时 14\.6 秒/);
  assert.match(content, /精确优化阶段已耗时，但没有已完成的精确优化换基记录/);
});

test('archived metadata missing new counters never renders invented zero counts', () => {
  for (const missing of [{}, {
    floating_search_pivots: null, optimization_pivots: null, improving_pivots: null,
    optimization_elapsed_seconds: null, first_feasible_seconds: null,
  }]) {
    const content = html({ optimization: { ...optimum, ...missing } });
    assert.match(content, /浮点\/可行性搜索换基 未记录计数；精确优化换基 未记录计数，其中成本下降 未记录计数/);
    assert.match(content, /精确优化阶段耗时 未记录/);
    assert.doesNotMatch(content, /换基 0|成本下降 0|首次已验证可行解耗时|没有已完成/);
  }
});

test('exact optimization count, improving count and recent history have distinct scopes', () => {
  const content = html({ optimization: {
    ...timeout, floating_search_pivots: 40, optimization_pivots: 14,
    improving_pivots: 6, optimization_elapsed_seconds: 20, first_feasible_seconds: null,
    recent_pivot_cost_changes: [{ pivot: 14, cost_before: 8, cost_after: 8, decrease: 0 }],
  } });
  assert.match(content, /精确优化换基 14，其中成本下降 6/);
  assert.match(content, /最近 1 次精确优化换基的成本变化/);
  assert.match(content, /第 14 次换基：8 → 8，下降 0/);
  assert.doesNotMatch(content, /没有已完成|首次已验证可行解耗时/);
});
