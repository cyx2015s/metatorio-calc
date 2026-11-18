// 所有已挂载悬停源共用一个关闭监听器，而非每个图标各挂一个。
const dismissers = new Set<() => void>();
export function dismissHover() { for (const dismiss of dismissers) dismiss(); }
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
