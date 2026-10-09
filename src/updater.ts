import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

const banner = document.querySelector<HTMLElement>("#launcher-update");
let pending: Update | null = null;
let installing = false;

// El cliente se está comprobando, descargando o lanzando: main.ts marca el body con is-busy.
function clientBusy(): boolean {
  return document.body.classList.contains("is-busy");
}

function render(message: string, actions: HTMLElement[] = []): void {
  if (!banner) return;
  banner.replaceChildren();
  const text = document.createElement("span");
  text.textContent = message;
  banner.append(text, ...actions);
  banner.hidden = false;
}

function button(label: string, onClick: () => void): HTMLButtonElement {
  const element = document.createElement("button");
  element.type = "button";
  element.className = "button button-secondary";
  element.textContent = label;
  element.addEventListener("click", onClick);
  return element;
}

function showAvailable(update: Update, hint = ""): void {
  const notes = update.body?.trim().split("\n")[0] ?? "";
  const message = `Nueva versión ${update.version} del launcher disponible${notes ? `: ${notes}` : ""}${hint}`;
  render(message, [
    button("Actualizar launcher", () => void install()),
    button("Más tarde", () => {
      if (banner) banner.hidden = true;
    }),
  ]);
}

async function install(): Promise<void> {
  if (!pending || installing) return;
  if (clientBusy()) {
    showAvailable(pending, " — termina antes la operación del cliente en curso.");
    return;
  }
  installing = true;
  let total = 0;
  let done = 0;
  try {
    render("Descargando la actualización del launcher…");
    await pending.downloadAndInstall((event) => {
      if (event.event === "Started") total = event.data.contentLength ?? 0;
      if (event.event === "Progress") {
        done += event.data.chunkLength;
        const percent = total > 0 ? ` ${Math.round((done / total) * 100)} %` : "";
        render(`Descargando la actualización del launcher…${percent}`);
      }
      if (event.event === "Finished") render("Instalando la actualización…");
    });
    await relaunch();
  } catch (error) {
    installing = false;
    const detail = error instanceof Error ? error.message : String(error);
    render(`No se pudo actualizar el launcher: ${detail}`, [button("Reintentar", () => void install())]);
  }
}

export async function checkLauncherUpdate(): Promise<void> {
  try {
    const update = await check();
    if (!update) return;
    pending = update;
    showAvailable(update);
  } catch (error) {
    // Sin red o sin release publicada: el launcher sigue funcionando.
    console.warn("No se pudo comprobar la actualización del launcher", error);
  }
}
