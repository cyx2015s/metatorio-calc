import { lookupName } from "../display-labels.ts";
import { richTextPlain } from "../rich-text.ts";
import type { CatalogIndex } from "./types.ts";

/** 本地化显示名索引与纯文本结果记忆化缓存（避免重复解析 Factorio 富文本）。 */
export class CatalogNameIndex {
  private rawMap = new Map<string, string>();
  private plainMap = new Map<string, string>();
  private index: CatalogIndex | null = null;
  private active = new Set<string>();

  /** 同步目录索引引用；引用变化或置空时重建原始映射并清空纯文本缓存。 */
  sync(catalogIndex: CatalogIndex | null): void {
    if (catalogIndex === this.index) return;
    if (!catalogIndex) {
      this.clear();
      return;
    }
    const raw = new Map<string, string>();
    for (const entry of catalogIndex.entries) {
      raw.set(`${entry.kind}/${entry.name}`, entry.localized_name);
    }
    // 非条目的名字：分组、类别、表面属性等，保留完整命名空间。
    for (const [name, localized] of Object.entries(catalogIndex.names ?? {})) {
      raw.set(name, localized);
    }
    this.rawMap = raw;
    this.plainMap.clear();
    this.active.clear();
    this.index = catalogIndex;
  }

  /** Raw Factorio markup, for RichText only. IPC/prototype IDs remain unchanged. */
  richName(kind: string, name: string, catalogIndex?: CatalogIndex | null): string {
    if (catalogIndex !== undefined) this.sync(catalogIndex);
    return lookupName(this.rawMap, kind, name);
  }

  /** Plain visible label, safe for titles, aria labels and ordinary controls. */
  localizedName(kind: string, name: string, catalogIndex?: CatalogIndex | null): string {
    if (catalogIndex !== undefined) this.sync(catalogIndex);
    const key = `${kind}/${name}`;
    const cached = this.plainMap.get(key);
    if (cached !== undefined) return cached;
    if (this.active.has(key)) return name;
    this.active.add(key);
    try {
      const plain = richTextPlain(this.richName(kind, name), (k, n) => this.localizedName(k, n)).trim();
      this.plainMap.set(key, plain);
      return plain;
    } finally {
      this.active.delete(key);
    }
  }

  /** 清空所有缓存状态与索引引用。 */
  clear(): void {
    this.rawMap.clear();
    this.plainMap.clear();
    this.active.clear();
    this.index = null;
  }
}
