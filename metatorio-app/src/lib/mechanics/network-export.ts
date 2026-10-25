// Explicit original dimensions avoid Cytoscape's implicit device-pixel-ratio
// multiplier. The renderer determines whether the actual canvas can be created.
export function networkPngSize(width: number, height: number) {
  if (!Number.isFinite(width) || !Number.isFinite(height) || width <= 0 || height <= 0) {
    throw new Error('网络图像边界无效，请先重排布局');
  }
  return {width: Math.ceil(width), height: Math.ceil(height)};
}
