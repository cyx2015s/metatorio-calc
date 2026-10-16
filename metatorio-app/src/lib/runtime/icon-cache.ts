type Loader = (type: string, name: string, context: string) => Promise<number[] | null>;

/** 合并并发请求；失败可重试，过期请求不会污染新一轮缓存。 */
export class IconCache {
  private entries = new Map<string, Promise<string | null>>();
  private urls: string[] = [];
  private generation = 0;

  private load: Loader;
  constructor(load: Loader) { this.load = load; }

  get(context: string, type: string, name: string): Promise<string | null> {
    if (!context || !type || !name) return Promise.resolve(null);
    const key = JSON.stringify([context, type, name]);
    const cached = this.entries.get(key);
    if (cached) return cached;
    const generation = this.generation;
    const discard = () => {
      if (this.entries.get(key) === pending) this.entries.delete(key);
      return null;
    };
    const pending = Promise.resolve().then(() => this.load(type, name, context))
      .then((bytes) => {
        if (generation !== this.generation || !bytes?.length) return discard();
        const url = URL.createObjectURL(new Blob([new Uint8Array(bytes)], { type: "image/png" }));
        this.urls.push(url);
        return url;
      })
      .catch(discard);
    this.entries.set(key, pending);
    return pending;
  }

  clear(): void {
    this.generation++;
    for (const url of this.urls) URL.revokeObjectURL(url);
    this.urls = [];
    this.entries.clear();
  }
}
