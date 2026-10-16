import { test } from "node:test";
import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import { IconCache } from "../src/lib/runtime/icon-cache.ts";
import { iconTypeForKind, syntheticIconUrls } from "../src/lib/ui/icon-types.ts";

test("rejected and empty icon requests can recover", async () => {
  let calls = 0;
  const cache = new IconCache(async () => {
    calls++;
    if (calls === 1) throw new Error("IPC unavailable");
    return calls === 2 ? null : [1, 2, 3];
  });
  assert.equal(await cache.get("ctx", "item", "iron"), null);
  assert.equal(await cache.get("ctx", "item", "iron"), null);
  assert.match(await cache.get("ctx", "item", "iron"), /^blob:/);
  assert.equal(calls, 3);
  cache.clear();
});

test("requests are deduplicated and isolated by context and type", async () => {
  let calls = 0;
  const cache = new IconCache(async () => { calls++; return [1]; });
  const a = cache.get("ctx", "item", "same");
  assert.equal(cache.get("ctx", "item", "same"), a);
  await a;
  await cache.get("other", "item", "same");
  await cache.get("ctx", "entity", "same");
  assert.equal(await cache.get("", "item", "same"), null);
  assert.equal(calls, 3);
  cache.clear();
});

test("an old request cannot delete or populate a newer cache generation", async () => {
  const pending = [];
  const cache = new IconCache(() => new Promise((resolve) => pending.push(resolve)));
  const old = cache.get("ctx", "tile", "water");
  await Promise.resolve();
  cache.clear();
  const fresh = cache.get("ctx", "tile", "water");
  await Promise.resolve();
  pending[0]([1]);
  assert.equal(await old, null);
  assert.equal(cache.get("ctx", "tile", "water"), fresh);
  pending[1]([2]);
  const url = await fresh;
  assert.match(url, /^blob:/);
  cache.clear();
  await assert.rejects(fetch(url));
});

test("prototype kinds map to their generated folders", () => {
  for (const kind of ["tile", "technology", "planet", "surface", "quality", "recipe", "fluid", "asteroid-chunk", "space-location"]) {
    assert.equal(iconTypeForKind(kind), kind);
  }
  assert.equal(iconTypeForKind("module"), "item");
  for (const kind of ["machine", "resource", "offshore-pump"]) assert.equal(iconTypeForKind(kind), "entity");
});

test("all synthetic icons have an existing static asset, including fluid fuel", () => {
  assert.equal(syntheticIconUrls.FluidFuel, "/icons/fluid-fuel.png");
  for (const path of Object.values(syntheticIconUrls)) {
    assert.ok(existsSync(new URL(`../static${path}`, import.meta.url)), path);
  }
});

