// Share observers across all row shells. At the narrow workspace breakpoint the
// workspace itself scrolls; on desktop each column owns its scroll position.
type Subscription = { root: Element | null; callback: (visible: boolean) => void };
const subscriptions = new Map<HTMLElement, Subscription>();
const roots = new Map<Element | null, { observer: IntersectionObserver; nodes: Set<HTMLElement> }>();
function rootFor(node: HTMLElement) {
  return node.closest(window.matchMedia("(max-width: 920px)").matches ? ".workspace" : ".center");
}
function attach(node: HTMLElement, subscription: Subscription) {
  let shared = roots.get(subscription.root);
  if (!shared) {
    const observer = new IntersectionObserver((entries) => {
      for (const entry of entries) subscriptions.get(entry.target as HTMLElement)?.callback(entry.isIntersecting);
    }, { root: subscription.root, rootMargin: "288px 0px" });
    shared = { observer, nodes: new Set() };
    roots.set(subscription.root, shared);
  }
  shared.nodes.add(node);
  shared.observer.observe(node);
}
function detach(node: HTMLElement, subscription: Subscription) {
  const shared = roots.get(subscription.root);
  if (!shared) return;
  shared.observer.unobserve(node);
  shared.nodes.delete(node);
  if (!shared.nodes.size) { shared.observer.disconnect(); roots.delete(subscription.root); }
}
function resize() {
  for (const [node, subscription] of subscriptions) {
    const root = rootFor(node);
    if (root === subscription.root) continue;
    detach(node, subscription);
    subscription.root = root;
    attach(node, subscription);
  }
}
export function observeRow(node: HTMLElement, callback: (visible: boolean) => void) {
  if (typeof IntersectionObserver === "undefined") { callback(true); return () => {}; }
  const subscription = { root: rootFor(node), callback };
  if (!subscriptions.size) window.addEventListener("resize", resize);
  subscriptions.set(node, subscription);
  attach(node, subscription);
  return () => {
    detach(node, subscription);
    subscriptions.delete(node);
    if (!subscriptions.size) window.removeEventListener("resize", resize);
  };
}
