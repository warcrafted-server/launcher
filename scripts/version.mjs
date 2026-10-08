#!/usr/bin/env node
// Uso: node scripts/version.mjs <X.Y.Z>   fija la versión en todos los archivos
//      node scripts/version.mjs --check   comprueba que todos coinciden
import { readFileSync, writeFileSync } from "node:fs";

const SEMVER = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(-[0-9A-Za-z.-]+)?$/;

const targets = [
  {
    file: "package.json",
    pattern: /("version":\s*")([^"]+)(")/,
  },
  {
    file: "src-tauri/tauri.conf.json",
    pattern: /("version":\s*")([^"]+)(")/,
  },
  {
    file: "src-tauri/Cargo.toml",
    pattern: /(\[package\][\s\S]*?\nversion\s*=\s*")([^"]+)(")/,
  },
  {
    file: "src-tauri/Cargo.lock",
    pattern: /(name = "warcrafted-launcher"\nversion = ")([^"]+)(")/,
  },
];

function current(target) {
  const match = readFileSync(target.file, "utf8").match(target.pattern);
  if (!match) throw new Error(`No se encontró la versión en ${target.file}`);
  return match[2];
}

const arg = process.argv[2];

if (arg === "--check") {
  const versions = targets.map((t) => [t.file, current(t)]);
  for (const [file, version] of versions) console.log(`${version}  ${file}`);
  const distinct = new Set(versions.map(([, v]) => v));
  if (distinct.size !== 1) {
    console.error("ERROR: las versiones no coinciden.");
    process.exit(1);
  }
  process.exit(0);
}

if (!arg || !SEMVER.test(arg)) {
  console.error("Uso: node scripts/version.mjs <X.Y.Z> | --check");
  process.exit(2);
}

for (const target of targets) {
  const text = readFileSync(target.file, "utf8");
  if (!target.pattern.test(text)) {
    throw new Error(`No se encontró la versión en ${target.file}`);
  }
  writeFileSync(target.file, text.replace(target.pattern, `$1${arg}$3`));
  console.log(`${target.file} -> ${arg}`);
}
