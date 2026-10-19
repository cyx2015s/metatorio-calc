export interface MechanicScrollState { top: number }

/** Save host scrolling independently for each view; avoid moving the whole page. */
export function rememberMechanicScroll(node: HTMLElement, state: MechanicScrollState) {
  const host = node.closest<HTMLElement>('.center');
  if (!host) return;
  const save = () => { state.top = host.scrollTop; };
  const frame = requestAnimationFrame(() => {
    host.scrollTop = state.top;
    host.addEventListener('scroll', save, {passive: true});
  });
  return {destroy() { cancelAnimationFrame(frame); host.removeEventListener('scroll', save); }};
}
