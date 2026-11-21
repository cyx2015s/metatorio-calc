// 所有已挂载悬停源共用一个关闭监听器，而非每个图标各挂一个。
// 同时共享最近悬停活跃时间戳：冷启动 150ms 意图延迟（扫过密集网格不闪烁），
// 300ms 内从上一个悬停切换时 0ms 热打开。
const COLD_OPEN_DELAY_MS = 150;
const WARM_SWITCH_WINDOW_MS = 300;

const dismissers = new Set<() => void>();
let activeToken: symbol | null = null;
let lastActiveAt = -Infinity;

export function hoverOpenDelay(): number {
  if (activeToken !== null) return 0;
  return Date.now() - lastActiveAt <= WARM_SWITCH_WINDOW_MS ? 0 : COLD_OPEN_DELAY_MS;
}

export function markHoverActive(token: symbol): void {
  activeToken = token;
  lastActiveAt = Date.now();
}

export function markHoverInactive(token: symbol): void {
  if (activeToken === token) {
    activeToken = null;
    lastActiveAt = Date.now();
  }
}

export function dismissHover() {
  for (const dismiss of dismissers) dismiss();
}

function pointerDown(event: PointerEvent) {
  if (!(event.target as Element)?.closest(".hover-card")) dismissHover();
}

export function registerHoverDismiss(dismiss: () => void) {
  if (!dismissers.size) document.addEventListener("pointerdown", pointerDown, true);
  dismissers.add(dismiss);
  return () => {
    dismissers.delete(dismiss);
    if (!dismissers.size) document.removeEventListener("pointerdown", pointerDown, true);
  };
}
