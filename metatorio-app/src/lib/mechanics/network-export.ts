// Bound both dimensions and area: universe layouts can be much larger than a
// browser/WebView canvas. Explicit dimensions also avoid device-pixel-ratio growth.
export const NETWORK_PNG_MAX_SIDE = 8192;
export const NETWORK_PNG_MAX_PIXELS = 16 * 1024 * 1024;

export function networkPngSize(width: number, height: number) {
  if (!Number.isFinite(width) || !Number.isFinite(height) || width <= 0 || height <= 0) {
    throw new Error('网络图像边界无效，请先重排布局');
  }
  const w = Math.ceil(width), h = Math.ceil(height);
  const scale = Math.min(1, NETWORK_PNG_MAX_SIDE / w, NETWORK_PNG_MAX_SIDE / h,
    Math.sqrt(NETWORK_PNG_MAX_PIXELS / w / h));
  return {width: Math.max(1, Math.floor(w * scale)), height: Math.max(1, Math.floor(h * scale)),
    scale, reduced: scale < 1};
}
