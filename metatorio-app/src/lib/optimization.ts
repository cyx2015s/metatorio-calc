import type { OptimizationReport, PivotCostChange, SolveDiagnostics } from "./runtime/types";

/** A writeback subset certificate cannot establish the full plan's optimum. */
export function resultOptimization(report?: SolveDiagnostics): OptimizationReport | null {
  return report?.planning_optimization ?? report?.optimization ?? null;
}

export function optimizationLabel(report: OptimizationReport): string {
  if (report.optimality_proven) return "已确认最优";
  if (report.termination_reason === "time-limit") {
    return "已到时限，返回已验证可行方案（未确认最优）";
  }
  return "未确认最优";
}

/** Zero decreases expose stalls; exact decreases can survive rounded cost ties. */
export function recentPivots(report: OptimizationReport): PivotCostChange[] {
  return report.recent_pivot_cost_changes.filter(change =>
    Number.isFinite(change.cost_before) && Number.isFinite(change.cost_after) &&
    Number.isFinite(change.decrease) && change.decrease >= 0,
  );
}
