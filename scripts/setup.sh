#!/usr/bin/env bash
# Prepara el entorno de desarrollo en Debian/Ubuntu. Uso: bash scripts/setup.sh
set -euo pipefail

if ! command -v apt-get >/dev/null; then
  echo "Este script solo soporta distribuciones con apt (Debian/Ubuntu)." >&2
  exit 1
fi

sudo apt-get update
sudo apt-get install -y build-essential curl wget file pkg-config nodejs npm \
  libwebkit2gtk-4.1-dev libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev

if ! command -v cargo >/dev/null && [ ! -x "$HOME/.cargo/bin/cargo" ]; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
fi
export PATH="$HOME/.cargo/bin:$PATH"

npm install

echo
node --version
npm --version
cargo --version
echo "Listo. Si cargo no se reconoce, abre una terminal nueva. Después: npm run tauri dev"
