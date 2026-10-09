#!/usr/bin/env python3
"""Genera THIRD-PARTY-NOTICES.md con las dependencias que se redistribuyen con el launcher."""
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def rust_components() -> list[dict]:
    raw = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--manifest-path", str(ROOT / "src-tauri/Cargo.toml")],
        check=True, capture_output=True, text=True,
    ).stdout
    meta = json.loads(raw)
    packages = {p["id"]: p for p in meta["packages"]}
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    root = meta["resolve"]["root"]
    seen, stack = set(), [root]
    while stack:
        current = stack.pop()
        if current in seen:
            continue
        seen.add(current)
        for dep in nodes[current]["deps"]:
            if any(k["kind"] is None for k in dep["dep_kinds"]):
                stack.append(dep["pkg"])
    seen.discard(root)
    result = []
    for pid in seen:
        p = packages[pid]
        result.append({
            "name": p["name"], "version": p["version"],
            "license": p["license"] or "SIN LICENCIA DECLARADA",
            "url": p["repository"] or p["homepage"] or f"https://crates.io/crates/{p['name']}",
        })
    return result


def npm_components() -> list[dict]:
    lock = json.loads((ROOT / "package-lock.json").read_text(encoding="utf-8"))
    package = json.loads((ROOT / "package.json").read_text(encoding="utf-8"))
    wanted = set(package.get("dependencies", {}))
    result = []
    for path, info in lock["packages"].items():
        name = path.removeprefix("node_modules/")
        if not path or name not in wanted:
            continue
        meta = json.loads((ROOT / path / "package.json").read_text(encoding="utf-8"))
        repo = meta.get("repository")
        url = repo.get("url") if isinstance(repo, dict) else repo
        result.append({
            "name": name, "version": info["version"],
            "license": meta.get("license", "SIN LICENCIA DECLARADA"),
            "url": (url or f"https://www.npmjs.com/package/{name}").removeprefix("git+").removesuffix(".git"),
        })
    return result


def table(rows: list[dict]) -> str:
    lines = ["| Componente | Versión | Licencia | Origen |", "| --- | --- | --- | --- |"]
    for r in sorted(rows, key=lambda r: r["name"].lower()):
        lines.append(f"| {r['name']} | {r['version']} | {r['license']} | {r['url']} |")
    return "\n".join(lines)


def main() -> int:
    rust, npm = rust_components(), npm_components()
    missing = [r["name"] for r in rust + npm if r["license"] == "SIN LICENCIA DECLARADA"]
    text = f"""# Avisos de terceros

Componentes de terceros que se redistribuyen con WarCrafted Launcher. Conservan su propia licencia
y avisos; quedan fuera del alcance de la licencia del proyecto (ver `LICENSE`). El texto completo
de cada licencia está en el código fuente de cada componente, en la URL indicada.

Generado con `scripts/third-party.py` a partir de `Cargo.lock` y `package-lock.json`. Regenéralo
al cambiar dependencias. No es asesoramiento jurídico.

## Rust (backend)

{table(rust)}

## JavaScript (interfaz)

{table(npm)}
"""
    (ROOT / "THIRD-PARTY-NOTICES.md").write_text(text, encoding="utf-8")
    print(f"{len(rust)} componentes Rust, {len(npm)} JavaScript")
    if missing:
        print("SIN LICENCIA DECLARADA:", ", ".join(missing), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
