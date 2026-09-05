#!/usr/bin/env node
// 同步应用版本号到所有版本来源文件。
// 用法: node scripts/sync-version.mjs major|minor|patch|<X.Y.Z>
// 读取 tauri.conf.json 的 version 作为当前版本来源，而后同步到：
//   metatorio-app/src-tauri/tauri.conf.json
//   metatorio-app/src-tauri/Cargo.toml
//   metatorio-app/package.json
//   Cargo.lock（metatorio-app 条目）
// 不触碰 workspace 其它 crate（metatorio-core/data/runtime/solver）的独立版本。
import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const CONF = resolve(root, "metatorio-app/src-tauri/tauri.conf.json");
const CARGO = resolve(root, "metatorio-app/src-tauri/Cargo.toml");
const PKG = resolve(root, "metatorio-app/package.json");
const LOCK = resolve(root, "Cargo.lock");

const target = process.argv[2];
if (!target) {
  console.error("用法: just version major|minor|patch|<X.Y.Z>");
  process.exit(1);
}

const conf = JSON.parse(readFileSync(CONF, "utf8"));
const current = conf.version;
if (!/^\d+\.\d+\.\d+$/.test(current)) {
  console.error(`当前版本格式不符: ${current}`);
  process.exit(1);
}
const [ma, mi, pa] = current.split(".").map((n) => Number(n));

let next;
if (target === "major") next = `${ma + 1}.0.0`;
else if (target === "minor") next = `${ma}.${mi + 1}.0`;
else if (target === "patch") next = `${ma}.${mi}.${pa + 1}`;
else if (/^\d+\.\d+\.\d+$/.test(target)) next = target;
else {
  console.error(`无效版本: ${target}（应为 major|minor|patch|<X.Y.Z>）`);
  process.exit(1);
}

if (next === current) {
  console.log(`版本未变: ${next}`);
  process.exit(0);
}

// tauri.conf.json（版本来源）
conf.version = next;
writeFileSync(CONF, JSON.stringify(conf, null, 2) + "\n");

// Cargo.toml —— [package] 的 version 行
let cargo = readFileSync(CARGO, "utf8");
cargo = cargo.replace(/^version\s*=\s*"[^"]*"/m, `version = "${next}"`);
writeFileSync(CARGO, cargo);

// package.json
const pkg = JSON.parse(readFileSync(PKG, "utf8"));
pkg.version = next;
writeFileSync(PKG, JSON.stringify(pkg, null, 2) + "\n");

// Cargo.lock —— metatorio-app 条目
let lock = readFileSync(LOCK, "utf8");
lock = lock.replace(
  /name = "metatorio-app"\nversion = "[^"]*"/,
  `name = "metatorio-app"\nversion = "${next}"`,
);
writeFileSync(LOCK, lock);

console.log(`版本 ${current} -> ${next}`);
