# Instalación

## Como jugador (Windows x64)

1. Descarga el instalador `WarCrafted Launcher_X.Y.Z_x64-setup.exe` de la última release del
   repositorio (cuando exista; hoy el instalador aún no se publica).
2. Ejecútalo. Se instala **solo para tu usuario**, sin permisos de administrador. Si falta
   WebView2, el instalador lo descarga e instala.
3. Abre «WarCrafted Launcher» desde el menú Inicio y elige la carpeta del cliente (o la carpeta
   donde quieres instalarlo).
4. Para desinstalar: «Aplicaciones» de Windows → WarCrafted Launcher. No toca tu cliente de WoW.

Windows SmartScreen puede avisar mientras el instalador no esté firmado con un certificado de
código (pendiente, ver `TODO.md`).

## Compilar el instalador (desarrolladores)

Requisitos: el entorno de desarrollo del [README](../README.md#preparar-el-entorno-automático) en
**Windows** (el instalador NSIS se compila en Windows; en Linux Tauri no puede generarlo).

```powershell
npm install
npm run tauri build
```

Salida: `src-tauri\target\release\bundle\nsis\WarCrafted Launcher_X.Y.Z_x64-setup.exe`.

El instalador incluye `LICENSE`, `CREDITS.md` y `THIRD-PARTY-NOTICES.md` junto al ejecutable. Si
cambian las dependencias, regenera este último con `python3 scripts/third-party.py` antes de
compilar. Los iconos salen de `src/assets/logo-warcrafted.jpg` con
`npx tauri icon src/assets/logo-warcrafted.jpg --fit contain` (el logotipo tiene su licencia
pendiente de verificar: no publiques el instalador hasta resolverlo).
