import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { factorioColor, normalizeSearch, parseRichText, richTextPlain } from "../src/lib/rich-text.ts";

const cases=JSON.parse(readFileSync(new URL("../../tests/fixtures/rich-text.json",import.meta.url),"utf8"));
for (const {source,plain,normalized} of cases) test(`plain projection: ${source.slice(0,75)}`,()=>{
  assert.equal(richTextPlain(source),plain);
  if (normalized != null) assert.equal(normalizeSearch(source),normalized);
});

test("nested modifiers restore their own previous state",()=>{
  const tokens=parseRichText("[color=red]A[font=default-bold][color=blue]B[/color]C[/font]D[/color]E");
  assert.deepEqual(tokens.map(t=>[t.text,t.color,!!t.bold]),[
    ["A","#ff0000",false],["B","#0000ff",true],["C","#ff0000",true],["D","#ff0000",false],["E",undefined,false]
  ]);
});
test("colors use controlled CSS values and Factorio alpha order",()=>{
  assert.equal(factorioColor("#80FF0000"),"#ff000080");
  assert.equal(factorioColor("1,0.5,0"),"rgb(255, 128, 0)");
  assert.equal(factorioColor("255,128,0"),"rgb(255, 128, 0)");
  for(const value of ["red;background:url(evil)","var(--x)","-1,0,0","999,0,0","#abcd","NaN,0,0"]) assert.equal(factorioColor(value),undefined);
});
test("reference labels resolve per context, never through cached resolved values",()=>{
  const source="[item=iron-plate,quality=rare]";
  assert.equal(richTextPlain(source,(kind,name)=>kind==='quality'?'稀有':'铁板')," 铁板 (稀有) ");
  assert.equal(richTextPlain(source,(kind,name)=>name)," iron-plate (rare) ");
  assert.equal(normalizeSearch(source,(kind,name)=>kind==='quality'?'稀有':'铁板'),"铁板(稀有)");
});
test("tags neither split Chinese words nor match color metadata",()=>{
  const source=cases[0].source;
  assert.ok(normalizeSearch(source).includes(normalizeSearch("量子点")));
  assert.ok(!normalizeSearch(source).includes("ff4040"));
  assert.equal(normalizeSearch("[color=red][/color]"),"");
});
test("large opaque blueprints and malformed bracket sequences stay linear",()=>{
  assert.equal(richTextPlain(`[special-item=${'x'.repeat(100000)}]`),"Blueprint");
  assert.equal(richTextPlain('['.repeat(100000)), '['.repeat(100000));
});
