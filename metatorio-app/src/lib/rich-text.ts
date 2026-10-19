/** Factorio text, never HTML. Keep the source intact; render only these tokens.
 * Unknown tags remain literal. Unsupported game actions degrade to readable text.
 */
export type RichStyle = { color?: string; bold?: boolean; mono?: boolean };
export type RichToken = RichStyle & (
  | { type: "text"; text: string }
  | { type: "icon"; kind: string; name: string; quality?: string }
);
export type NameResolver = (kind: string, name: string) => string;

const references = new Set(["item", "entity", "technology", "recipe", "item-group", "fluid", "tile", "virtual-signal", "achievement", "quality", "planet", "space-location", "shortcut"]);
const spriteClasses = new Set([...references, "equipment", "ammo-category", "decorative", "space-connection", "surface", "airborne-pollutant", "asteroid-chunk", "utility"]);
const colors: Record<string, string> = { red: "#ff0000", green: "#00ff00", blue: "#0000ff", orange: "#ff8000", yellow: "#ffff00", pink: "#ff80ff", purple: "#800080", white: "#ffffff", black: "#000000", gray: "#808080", brown: "#996633", cyan: "#00ffff", acid: "#bfff00" };

export function factorioColor(value: string): string | undefined {
  const input = value.trim().toLowerCase();
  if (Object.hasOwn(colors, input)) return colors[input];
  if (/^#[\da-f]{6}$/.test(input)) return input;
  // Factorio uses AARRGGBB; CSS uses RRGGBBAA.
  if (/^#[\da-f]{8}$/.test(input)) return `#${input.slice(3)}${input.slice(1, 3)}`;
  if (!/^\d*\.?\d+\s*,\s*\d*\.?\d+\s*,\s*\d*\.?\d+$/.test(input)) return;
  const rgb = input.split(",").map(Number);
  const max = rgb.some((n) => n > 1) ? 255 : 1;
  if (rgb.some((n) => !Number.isFinite(n) || n < 0 || n > max)) return;
  return `rgb(${rgb.map((n) => Math.round(n / max * 255)).join(", ")})`;
}

function validName(name: string): boolean {
  return !!name && name.length <= 1024 && name !== "." && name !== ".." && !/[\s,\[\]\/\\]/.test(name);
}

const cache = new Map<string, readonly RichToken[]>();
export function parseRichText(source: string): readonly RichToken[] {
  if (!source.includes("[")) return source ? [{ type: "text", text: source }] : [];
  const cached = cache.get(source);
  if (cached) return cached;
  const result: RichToken[] = [];
  const colorStack: (string | undefined)[] = [];
  const fontStack: string[] = [];
  const style = (): RichStyle => ({ color: colorStack.at(-1), bold: fontStack.at(-1)?.includes("bold"), mono: fontStack.at(-1)?.includes("mono") });
  const text = (text: string) => { if (text) result.push({ type: "text", text, ...style() }); };
  let end = 0;
  // Excluding nested brackets makes scanning linear, including large blueprints.
  for (const match of source.matchAll(/\[([^\[\]\r\n]+)\]/g)) {
    text(source.slice(end, match.index));
    end = match.index! + match[0].length;
    const body = match[1];
    const equals = body.indexOf("=");
    const tag = equals < 0 ? body : body.slice(0, equals);
    const value = equals < 0 ? "" : body.slice(equals + 1);
    if (tag === "color" && equals >= 0) colorStack.push(factorioColor(value));
    else if (tag === "font" && equals >= 0) fontStack.push(value);
    else if (equals < 0 && (tag === "/color" || tag === ".color")) colorStack.pop();
    else if (equals < 0 && (tag === "/font" || tag === ".font")) fontStack.pop();
    else if (references.has(tag) && equals >= 0) {
      const [name, ...parameters] = value.split(",");
      const quality = ["item", "entity", "recipe", "fluid", "virtual-signal"].includes(tag)
        ? parameters.find((p) => p.startsWith("quality="))?.slice(8) : undefined;
      if (validName(name) && (!quality || validName(quality))) result.push({ type: "icon", kind: tag, name, quality, ...style() });
      else text(match[0]);
    } else if (tag === "img" && equals >= 0) {
      const split = value.search(/[/.]/);
      const kind = value.slice(0, split), name = value.slice(split + 1);
      if (split > 0 && spriteClasses.has(kind) && validName(name)) result.push({ type: "icon", kind, name, ...style() });
      else text(value); // Arbitrary sprite prototypes are unavailable in the icon cache.
    } else if (tag === "tooltip" && value.includes(",")) text(value.slice(0, value.lastIndexOf(",")));
    else if (tag === "gps" && equals >= 0) text(`GPS ${value}`);
    else if (["train", "train-stop", "space-platform", "armor", "tip"].includes(tag) && equals >= 0) text(`${tag}: ${value}`);
    else if (tag === "special-item" && equals >= 0) text("Blueprint");
    else if (body === "space-age") text("Space Age");
    else text(match[0]);
  }
  text(source.slice(end));
  // Bounded and source-only: resolved names/icons must never leak across contexts.
  if (source.length <= 16384) {
    if (cache.size >= 2048) cache.delete(cache.keys().next().value!);
    cache.set(source, result);
  }
  return result;
}

export function richTextPlain(source: string, resolve?: NameResolver): string {
  return parseRichText(source).map((token) => token.type === "text" ? token.text
    : ` ${resolve?.(token.kind, token.name) || token.name}${token.quality && token.quality !== "normal" ? ` (${resolve?.("quality", token.quality) || token.quality})` : ""} `).join("");
}

/** Same separator/case policy as the Rust MCP catalog matcher. */
export function normalizeSearch(source: string, resolve?: NameResolver): string {
  return richTextPlain(source, resolve)
    .replace(/[\uff01-\uff5e]/g, (c) => String.fromCharCode(c.charCodeAt(0) - 0xfee0))
    .toLowerCase().replace(/[\s_\-\u2010-\u2015\u2212\u0085\u00ad\u200b-\u200d\u2060\ufeff]/gu, "");
}
