// 数量格式化（复刻原始 egui factorio/format.rs 的 compact_number）：
// - 极小值用 p（1e-12）/ μ（1e-6）
// - 大数用 k/M/G/T/P/E/Z/Y/R/Q 前缀
// - 无前缀时按数量级决定小数位（<10 两位、<1000 一位、更大取整）

const LARGE_UNITS = ["", "k", "M", "G", "T", "P", "E", "Z", "Y", "R", "Q"];

function trimDecimal(text: string): string {
  const trimmed = text.replace(/0+$/, "").replace(/\.$/, "");
  return trimmed === "-0" ? "0" : trimmed;
}

function formatWithUnit(value: number, unit: string): string {
  const abs = Math.abs(value);
  let formatted: string;
  if (abs < 0.01) {
    formatted = trimDecimal(value.toFixed(4));
  } else if (abs < 10) {
    formatted = trimDecimal(value.toFixed(2));
  } else if (abs < 100) {
    formatted = trimDecimal(value.toFixed(1));
  } else {
    formatted = String(Math.round(value));
  }
  if (formatted === "0" || formatted === "-0") return "0";
  return `${formatted}${unit}`;
}

/** 数量 → 紧凑文本（复刻 egui compact_number；0.001..0.01 输出小数，不输出毫 m 后缀）。 */
export function compactNumber(num: number | null): string {
  if (num == null || !Number.isFinite(num)) return "—";
  const abs = Math.abs(num);
  if (abs < 1e-15) return "0";
  if (abs < 1e-9) return formatWithUnit(num * 1e12, "p");
  if (abs < 0.001) return formatWithUnit(num * 1e6, "μ");
  if (abs < 0.01) return trimDecimal(num.toFixed(4));
  let n = abs;
  let unitIdx = 0;
  if (n > 10000) {
    while (n > 1000 && unitIdx < LARGE_UNITS.length - 1) {
      unitIdx += 1;
      n /= 1000;
    }
  }
  return formatWithUnit(n * Math.sign(num), LARGE_UNITS[unitIdx]);
}

/** 带符号数量（正数加 + 前缀；零返回 0）。 */
export function signedCompactNumber(num: number): string {
  if (!Number.isFinite(num)) return "—";
  if (Math.abs(num) < 1e-15) return "0";
  const text = compactNumber(Math.abs(num));
  if (text === "0") return "0";
  return num < 0 ? `-${text}` : `+${text}`;
}

/** 解析紧凑数量文本（p/μ/k/M/G/T…），失败返回 null。 */
export function parseCompactNumber(text: string): number | null {
  const match = /^([+-]?\d*\.?\d+)\s*([pμkMGTPEZYRQ])?$/i.exec(text.trim());
  if (!match) return null;
  const value = Number(match[1]);
  if (!Number.isFinite(value)) return null;
  const unit = (match[2] ?? "").toLowerCase();
  const multipliers: Record<string, number> = {
    p: 1e-12,
    μ: 1e-6,
    k: 1e3,
    m: 1e6,
    g: 1e9,
    t: 1e12,
    e: 1e18,
    z: 1e21,
    y: 1e24,
    r: 1e27,
    q: 1e30,
  };
  return value * (multipliers[unit] ?? 1);
}
