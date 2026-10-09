import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ask } from "@tauri-apps/plugin-dialog";

/**
 * Sección «Opcionales» de la pestaña Addons.
 *
 * Los addons opcionales los elige el jugador: nunca se confunden en la interfaz con los
 * obligatorios (que gestiona el sistema de integridad y no se pueden desactivar) y no influyen
 * ni en el botón Jugar ni en el resumen del cliente. Este módulo solo habla con los comandos
 * del backend y con `host`, que es quien conoce el estado del resto del launcher.
 */

/** Estado derivado de un addon opcional, tal como lo calcula el backend (`AddonInstallState`). */
export type AddonInstallState = "notInstalled" | "installed" | "updateAvailable";

/** Ficha de un addon opcional devuelta por `list_optional_addons`. */
export interface OptionalAddon {
  id: string;
  name: string;
  description: string;
  author: string;
  version: string;
  license: string;
  homepage: string;
  sizeBytes: number;
  folders: string[];
  state: AddonInstallState;
  installedVersion: string | null;
}

/** Catálogo firmado devuelto por `list_optional_addons`. */
export interface OptionalAddonsCatalog {
  catalogVersion: number;
  publishedAt: string;
  addons: OptionalAddon[];
}

/** Evento `download-progress` del backend. */
interface DownloadProgressPayload {
  bytesDone: number;
  bytesTotal: number;
  speedBytesPerSec: number;
  etaSeconds: number | null;
}

/** Puntos de unión con el launcher: estado del cliente, bloqueo global y avisos. */
export interface OptionalAddonsHost {
  /** Bloquea (o desbloquea) comprobar, actualizar y jugar mientras hay una operación de addon. */
  setAddonOperationActive(active: boolean): void;
  clientState(): { hasClientDirectory: boolean; clientBusy: boolean; gameRunning: boolean };
  notify(message: string, kind: "success" | "error" | "warning" | "info"): void;
}

type CatalogPhase = "idle" | "loading" | "ready" | "error";
type MessageKind = "success" | "error" | "warning" | "info";

/** Texto de `InstallError::ExistingFolder`: el backend pide confirmación para sustituir carpetas. */
const EXISTING_FOLDER_MARKER = "no la gestiona el launcher";
const CANCELLED_MESSAGE = "Operación cancelada.";

let root: HTMLElement | null = null;
let host: OptionalAddonsHost | null = null;
let list: HTMLUListElement | null = null;
let stateBox: HTMLElement | null = null;
let stateMessage: HTMLParagraphElement | null = null;
let stateButton: HTMLButtonElement | null = null;
let refreshButton: HTMLButtonElement | null = null;
let catalogMeta: HTMLSpanElement | null = null;
let progressArea: HTMLElement | null = null;
let progressBar: HTMLProgressElement | null = null;
let progressText: HTMLSpanElement | null = null;
let cancelButton: HTMLButtonElement | null = null;

let phase: CatalogPhase = "idle";
let addons: OptionalAddon[] = [];
let catalogError = "";
let loadedOnce = false;
let loadGeneration = 0;
let lastHasClientDirectory: boolean | null = null;
let lastClientBusy: boolean | null = null;
let operationId: string | null = null;
let canceling = false;
let actionButtons: HTMLButtonElement[] = [];
let stopProgressListening: (() => void) | undefined;

/** Monta el contenido de la sección «Opcionales» en su contenedor y enlaza con el launcher. */
export function initializeOptionalAddons(container: HTMLElement, actions: OptionalAddonsHost): void {
  root = container;
  host = actions;
  root.replaceChildren();
  buildLayout(root);
  watchAddonsPanel();
  refreshOptionalAddons();
}

/**
 * El catálogo se carga la primera vez que la pestaña Addons queda visible. Se observa el panel en
 * lugar del clic para cubrir también la navegación por teclado y la restauración de pestaña.
 */
function watchAddonsPanel(): void {
  const panel = document.getElementById("panel-addons");
  if (!panel) return;
  new MutationObserver(() => ensureCatalog()).observe(panel, { attributes: true, attributeFilter: ["hidden"] });
}

/**
 * Ajusta la sección al estado del cliente (carpeta elegida, operación del cliente en curso, juego
 * abierto). El catálogo se revalida cuando cambia la carpeta del cliente o cuando termina una
 * operación del cliente: el estado de cada addon se calcula contra esa carpeta, así que un cambio
 * lo invalida por completo.
 */
export function refreshOptionalAddons(): void {
  if (!root || !host) return;
  const { hasClientDirectory, clientBusy } = host.clientState();
  const directoryChanged = lastHasClientDirectory !== null && lastHasClientDirectory !== hasClientDirectory;
  const clientWorkFinished = lastClientBusy === true && !clientBusy;
  lastHasClientDirectory = hasClientDirectory;
  lastClientBusy = clientBusy;

  if (!hasClientDirectory) {
    invalidateCatalog();
    renderList();
    return;
  }
  if (directoryChanged || clientWorkFinished) invalidateCatalog();
  ensureCatalog();
  updateActionStates();
}

/** Descarta el catálogo en memoria: deja la sección lista para volver a pedirlo. */
function invalidateCatalog(): void {
  phase = "idle";
  addons = [];
  catalogError = "";
  loadedOnce = false;
  loadGeneration += 1;
}

function buildLayout(container: HTMLElement): void {
  const note = element(
    "p",
    "optional-note",
    "No son necesarios para jugar: instálalos solo si los quieres. A diferencia de los obligatorios, "
      + "puedes quitarlos cuando quieras.",
  );

  const toolbar = element("div", "optional-addons-toolbar");
  catalogMeta = element("span", "optional-catalog-meta");
  refreshButton = createButton("button button-secondary optional-refresh-button", "Actualizar lista", () =>
    void loadCatalog(),
  );
  toolbar.append(catalogMeta, refreshButton);

  stateBox = element("div", "optional-state");
  stateMessage = element("p", "optional-empty-state");
  stateButton = createButton("button button-secondary", "Reintentar", () => void loadCatalog());
  stateButton.hidden = true;
  stateBox.append(stateMessage, stateButton);

  list = element("ul", "optional-addon-list");
  list.hidden = true;

  progressArea = element("div", "optional-progress");
  progressArea.hidden = true;
  progressBar = element("progress", "optional-progress-bar");
  progressBar.max = 1;
  progressBar.value = 0;
  progressText = element("span", "optional-progress-text");
  cancelButton = createButton("button button-secondary optional-cancel-button", "Cancelar", () =>
    void requestCancel(),
  );
  progressArea.append(progressBar, progressText, cancelButton);

  container.append(note, toolbar, stateBox, list, progressArea);
}

/** Carga el catálogo la primera vez que se abre la pestaña Addons (nunca al arrancar). */
function ensureCatalog(): void {
  // Sin reintento automático tras un error: para eso están «Reintentar» y «Actualizar lista».
  if (phase === "loading" || phase === "error" || loadedOnce) return;
  if (!isAddonsTabVisible()) return;
  void loadCatalog();
}

function isAddonsTabVisible(): boolean {
  const panel = document.getElementById("panel-addons");
  return panel !== null && !panel.hidden;
}

async function loadCatalog(): Promise<void> {
  if (!host) return;
  if (!host.clientState().hasClientDirectory) {
    phase = "idle";
    renderList();
    return;
  }
  if (phase === "loading") return;
  const generation = loadGeneration;
  phase = "loading";
  catalogError = "";
  renderList();
  try {
    const catalog = await invoke<OptionalAddonsCatalog>("list_optional_addons");
    // La carpeta del cliente o su estado pueden cambiar mientras la petición está en vuelo: en ese
    // caso la respuesta ya no describe la situación actual y se descarta.
    if (generation !== loadGeneration) return;
    addons = catalog.addons;
    loadedOnce = true;
    phase = "ready";
    if (catalogMeta) catalogMeta.textContent = `Catálogo v${catalog.catalogVersion}`;
  } catch (error) {
    if (generation !== loadGeneration) return;
    phase = "error";
    catalogError = errorMessageOf(error);
    addons = [];
  }
  renderList();
}

function renderList(): void {
  if (!root || !list || !host) return;
  actionButtons = [];
  list.replaceChildren();
  const { hasClientDirectory } = host.clientState();

  if (!hasClientDirectory) {
    showState("Elige la carpeta del cliente en «Ajustes» para poder instalar addons opcionales.");
  } else if (phase === "idle" || phase === "loading") {
    showState("Cargando el catálogo de addons…");
  } else if (phase === "error") {
    showState(`No se pudo cargar el catálogo de addons: ${catalogError}`, true);
  } else if (addons.length === 0) {
    showState("Aún no hay addons opcionales publicados.");
  } else {
    hideState();
    for (const addon of addons) list.append(createAddonRow(addon));
  }
  updateActionStates();
}

function showState(message: string, retry = false): void {
  if (stateBox) stateBox.hidden = false;
  if (stateMessage) stateMessage.textContent = message;
  if (stateButton) stateButton.hidden = !retry;
  if (list) list.hidden = true;
}

function hideState(): void {
  if (stateBox) stateBox.hidden = true;
  if (list) list.hidden = false;
}

function createAddonRow(addon: OptionalAddon): HTMLLIElement {
  const row = element("li", "addon-row optional-addon-row");

  const details = element("div", "addon-details");
  details.append(element("strong", "", addon.name));
  details.append(element("span", "optional-label", "Opcional · no es necesario para jugar"));
  details.append(element("span", "addon-meta", `Por ${addon.author}`));
  details.append(element("span", "addon-meta", versionSummary(addon)));
  details.append(
    element("span", "addon-meta", `Licencia ${addon.license} · ${formatMegabytes(addon.sizeBytes)}`),
  );
  details.append(element("p", "addon-description", addon.description));
  if (isHttpsUrl(addon.homepage)) {
    details.append(createButton("text-button addon-link", "Sitio web ↗", () => openHomepage(addon.homepage)));
  }

  const aside = element("div", "addon-aside");
  const status = element("span", "addon-status", stateLabel(addon.state));
  status.dataset.state = stateTone(addon.state);
  aside.append(status);

  if (addon.state === "notInstalled") {
    aside.append(addActionButton("Instalar", () => void installAddon(addon)));
  } else if (addon.state === "installed") {
    aside.append(addActionButton("Desinstalar", () => void uninstallAddon(addon)));
  } else {
    aside.append(addActionButton("Actualizar", () => void installAddon(addon)));
    aside.append(addActionButton("Desinstalar", () => void uninstallAddon(addon)));
  }

  row.append(details, aside);
  return row;
}

function addActionButton(text: string, onClick: () => void): HTMLButtonElement {
  const node = createButton("button button-secondary addon-action-button", text, onClick);
  actionButtons.push(node);
  return node;
}

function updateActionStates(): void {
  if (!host) return;
  const state = host.clientState();
  const blocked = operationId !== null || state.clientBusy || state.gameRunning || !state.hasClientDirectory;
  const reason = operationId !== null
    ? "Espera a que termine la operación en curso"
    : state.gameRunning
      ? "Cierra el juego antes de gestionar los addons"
      : state.clientBusy
        ? "Espera a que termine la operación del cliente"
        : !state.hasClientDirectory
          ? "Elige primero la carpeta del cliente"
          : "";
  for (const button of actionButtons) {
    button.disabled = blocked;
    button.title = reason;
  }
  const controlBlocked = operationId !== null || state.clientBusy;
  if (refreshButton) {
    refreshButton.disabled = controlBlocked || !state.hasClientDirectory;
    refreshButton.title = controlBlocked
      ? "Espera a que termine la operación en curso"
      : "Volver a descargar el catálogo de addons";
  }
  if (stateButton) stateButton.disabled = controlBlocked;
  if (cancelButton) {
    cancelButton.disabled = canceling;
    cancelButton.textContent = canceling ? "Cancelando…" : "Cancelar";
  }
}

async function installAddon(addon: OptionalAddon): Promise<void> {
  if (!host || !canOperate()) return;
  const updating = addon.state === "updateAvailable";
  const verb = updating ? "actualizar" : "instalar";
  beginOperation(addon.id, "Preparando la descarga…");
  try {
    stopProgressListening = await listen<DownloadProgressPayload>("download-progress", ({ payload }) => {
      showDownloadProgress(payload);
    });
    if (updating) {
      // `update_optional_addon` solo admite addons registrados y autoriza sustituir sus carpetas.
      await invoke<void>("update_optional_addon", { id: addon.id });
    } else if (!(await installFreshAddon(addon))) {
      return;
    }
    host.notify(`Se ha ${updating ? "actualizado" : "instalado"} «${addon.name}».`, "success");
    await loadCatalog();
  } catch (error) {
    notifyOperationError(`No se pudo ${verb} «${addon.name}»`, error);
  } finally {
    endOperation();
  }
}

/**
 * Instala un addon aún no registrado. Si el backend rechaza la instalación porque la carpeta ya
 * existe y no la gestiona el launcher, pide confirmación y reintenta con `replaceExisting`.
 * Devuelve `false` si el jugador decide no continuar.
 */
async function installFreshAddon(addon: OptionalAddon): Promise<boolean> {
  try {
    await invoke<void>("install_optional_addon", { id: addon.id, replaceExisting: false });
    return true;
  } catch (error) {
    const message = errorMessageOf(error);
    if (!message.includes(EXISTING_FOLDER_MARKER)) throw error;
    const confirmed = await ask(
      `La carpeta de «${addon.name}» ya existe en tu cliente y no la instaló WarCrafted. `
        + "Se sustituirá por la versión del catálogo firmado. ¿Quieres continuar?",
      { title: "Carpeta existente", kind: "warning" },
    );
    if (!confirmed) {
      host?.notify("Instalación cancelada.", "info");
      return false;
    }
    await invoke<void>("install_optional_addon", { id: addon.id, replaceExisting: true });
    return true;
  }
}

async function uninstallAddon(addon: OptionalAddon): Promise<void> {
  if (!host || !canOperate()) return;
  const folders = addon.folders.length > 0
    ? ` Se borrarán las carpetas ${addon.folders.map((folder) => `«${folder}»`).join(", ")}.`
    : "";
  const confirmed = await ask(
    `Se desinstalará «${addon.name}».${folders} WarCrafted solo borra las carpetas que instaló. ¿Continuar?`,
    { title: "Desinstalar addon", kind: "warning" },
  );
  if (!confirmed) return;

  beginOperation(addon.id, "Desinstalando…");
  try {
    await invoke<void>("uninstall_optional_addon", { id: addon.id });
    host.notify(`Se ha desinstalado «${addon.name}».`, "success");
    await loadCatalog();
  } catch (error) {
    notifyOperationError(`No se pudo desinstalar «${addon.name}»`, error);
  } finally {
    endOperation();
  }
}

async function requestCancel(): Promise<void> {
  if (!host || operationId === null || canceling) return;
  canceling = true;
  updateActionStates();
  try {
    await invoke<void>("cancel_operation");
  } catch (error) {
    canceling = false;
    updateActionStates();
    host.notify(`No se pudo cancelar la operación: ${errorMessageOf(error)}`, "error");
  }
}

function beginOperation(id: string, label: string): void {
  operationId = id;
  canceling = false;
  if (progressArea) progressArea.hidden = false;
  if (progressBar) {
    progressBar.max = 1;
    progressBar.value = 0;
  }
  if (progressText) progressText.textContent = label;
  host?.setAddonOperationActive(true);
  updateActionStates();
}

function endOperation(): void {
  stopProgressListening?.();
  stopProgressListening = undefined;
  operationId = null;
  canceling = false;
  if (progressArea) progressArea.hidden = true;
  if (progressBar) {
    progressBar.max = 1;
    progressBar.value = 0;
  }
  if (progressText) progressText.textContent = "";
  host?.setAddonOperationActive(false);
  updateActionStates();
}

function showDownloadProgress(payload: DownloadProgressPayload): void {
  if (!progressArea || !progressBar || !progressText) return;
  const total = Math.max(payload.bytesTotal, 1);
  progressArea.hidden = false;
  progressBar.max = total;
  progressBar.value = Math.min(Math.max(payload.bytesDone, 0), total);
  const parts = [`${formatMegabytes(payload.bytesDone)} de ${formatMegabytes(payload.bytesTotal)}`];
  if (payload.speedBytesPerSec > 0) parts.push(formatSpeed(payload.speedBytesPerSec));
  if (payload.etaSeconds !== null) parts.push(formatEta(payload.etaSeconds));
  progressText.textContent = parts.join(" · ");
}

function canOperate(): boolean {
  if (!host || operationId !== null || canceling) return false;
  const state = host.clientState();
  return state.hasClientDirectory && !state.clientBusy && !state.gameRunning;
}

function notifyOperationError(prefix: string, error: unknown): void {
  const message = errorMessageOf(error);
  const kind: MessageKind = message === CANCELLED_MESSAGE ? "info" : "error";
  host?.notify(message === CANCELLED_MESSAGE ? "Operación cancelada." : `${prefix}: ${message}`, kind);
}

function openHomepage(url: string): void {
  window.dispatchEvent(new CustomEvent("warcrafted-open-url", { detail: url }));
}

function stateLabel(state: AddonInstallState): string {
  switch (state) {
    case "notInstalled": return "No instalado";
    case "installed": return "Instalado";
    case "updateAvailable": return "Actualización disponible";
  }
}

function stateTone(state: AddonInstallState): string {
  switch (state) {
    case "notInstalled": return "muted";
    case "installed": return "ok";
    case "updateAvailable": return "attention";
  }
}

function versionSummary(addon: OptionalAddon): string {
  if (addon.state === "updateAvailable" && addon.installedVersion) {
    return `Versión ${addon.version} · instalada ${addon.installedVersion}`;
  }
  const installed = addon.state === "installed" ? " · instalada" : "";
  return `Versión ${addon.version}${installed}`;
}

function isHttpsUrl(value: string): boolean {
  try {
    return new URL(value).protocol === "https:";
  } catch {
    return false;
  }
}

function formatMegabytes(bytes: number): string {
  if (bytes >= 1024 ** 3) {
    return `${(bytes / 1024 ** 3).toLocaleString("es-ES", { maximumFractionDigits: 1 })} GB`;
  }
  return `${(bytes / 1024 ** 2).toLocaleString("es-ES", { maximumFractionDigits: 1 })} MB`;
}

function formatSpeed(bytesPerSecond: number): string {
  if (bytesPerSecond >= 1024 ** 2) {
    return `${(bytesPerSecond / 1024 ** 2).toLocaleString("es-ES", { maximumFractionDigits: 1 })} MB/s`;
  }
  return `${Math.max(1, Math.round(bytesPerSecond / 1024)).toLocaleString("es-ES")} KB/s`;
}

function formatEta(seconds: number): string {
  if (seconds < 60) return `${seconds} s`;
  return `${Math.ceil(seconds / 60).toLocaleString("es-ES")} min`;
}

function errorMessageOf(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return "Se produjo un error inesperado.";
}

function element<K extends keyof HTMLElementTagNameMap>(tag: K, className = "", text = ""): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text) node.textContent = text;
  return node;
}

function createButton(className: string, text: string, onClick: () => void): HTMLButtonElement {
  const node = document.createElement("button");
  node.type = "button";
  node.className = className;
  node.textContent = text;
  node.addEventListener("click", onClick);
  return node;
}

