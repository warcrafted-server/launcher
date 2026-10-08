import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ask, open } from "@tauri-apps/plugin-dialog";

type ClientFileState = "ok" | "missing" | "corrupt" | "error";
type FileRole = "required" | "optional";
type UpdateFileState = "ok" | "error";

interface ClientFileStatus {
  path: string;
  role: FileRole;
  status: ClientFileState;
  message: string | null;
}

interface UpdateProgressPayload {
  path: string;
  index: number;
  total: number;
  status: UpdateFileState;
  message: string | null;
}

interface LauncherSettings {
  clientDir: string | null;
}

const checkButton = requiredElement<HTMLButtonElement>("#check-button");
const updateButton = requiredElement<HTMLButtonElement>("#update-button");
const playButton = requiredElement<HTMLButtonElement>("#play-button");
const clearCacheButton = requiredElement<HTMLButtonElement>("#clear-cache-button");
const chooseFolderButton = requiredElement<HTMLButtonElement>("#choose-folder-button");
const clientLocation = requiredElement<HTMLSpanElement>("#client-location");
const clientFolderHelp = requiredElement<HTMLParagraphElement>("#client-folder-help");
const operationMessage = requiredElement<HTMLDivElement>("#operation-message");
const progressSection = requiredElement<HTMLElement>("#progress-section");
const progressTitle = requiredElement<HTMLHeadingElement>("#progress-title");
const progressCount = requiredElement<HTMLSpanElement>("#progress-count");
const progressBar = requiredElement<HTMLProgressElement>("#update-progress");
const progressDetail = requiredElement<HTMLParagraphElement>("#progress-detail");
const fileList = requiredElement<HTMLUListElement>("#file-list");
const fileCount = requiredElement<HTMLSpanElement>("#file-count");

let clientFiles: ClientFileStatus[] = [];
let clientDir: string | null = null;
let hasCheckedClient = false;
let isChecking = false;
let isUpdating = false;
let isLaunching = false;
let isClearingCache = false;
let isLoadingSettings = true;

chooseFolderButton.addEventListener("click", () => void chooseClientFolder());
checkButton.addEventListener("click", () => void checkClient());
updateButton.addEventListener("click", () => void updateClient());
playButton.addEventListener("click", () => void launchGame());
clearCacheButton.addEventListener("click", () => void clearCache());

void loadSettings();

function requiredElement<T extends HTMLElement>(selector: string): T {
  const element = document.querySelector<T>(selector);
  if (!element) {
    throw new Error(`No se encontró el elemento de interfaz ${selector}.`);
  }
  return element;
}

function refreshButtons(): void {
  const busy = isLoadingSettings || isChecking || isUpdating || isLaunching || isClearingCache;
  const hasClientDirectory = clientDir !== null;
  chooseFolderButton.disabled = busy;
  checkButton.disabled = busy || !hasClientDirectory;
  updateButton.disabled = busy || !hasClientDirectory;
  clearCacheButton.disabled = busy || !hasClientDirectory;
  playButton.disabled = busy || !hasClientDirectory || !hasCheckedClient;
}

async function loadSettings(): Promise<void> {
  try {
    const settings = await invoke<LauncherSettings>("get_settings");
    clientDir = settings.clientDir;
    renderClientDirectory();
  } catch (error: unknown) {
    showMessage(`No se pudieron cargar los ajustes: ${errorMessage(error)}`, "error");
  } finally {
    isLoadingSettings = false;
    refreshButtons();
  }
}

async function chooseClientFolder(): Promise<void> {
  if (isLoadingSettings || isChecking || isUpdating || isLaunching || isClearingCache) return;

  try {
    const selectedPath = await open({
      directory: true,
      multiple: false,
      title: "Elige la carpeta del cliente de WoW",
    });
    if (selectedPath === null) return;

    const savedPath = await invoke<string>("set_client_dir", { path: selectedPath });
    clientDir = savedPath;
    hasCheckedClient = false;
    clientFiles = [];
    renderClientDirectory();
    fileList.replaceChildren(createEmptyState("Comprueba el estado para ver los archivos del cliente."));
    fileCount.textContent = "Aún no comprobados";
    progressSection.hidden = true;
    showMessage("La carpeta del cliente se ha guardado.", "success");
    refreshButtons();
  } catch (error: unknown) {
    showMessage(`No se pudo elegir la carpeta del cliente: ${errorMessage(error)}`, "error");
  }
}

function renderClientDirectory(): void {
  const path = clientDir ?? "Ninguna carpeta elegida";
  clientLocation.textContent = path;
  clientLocation.title = clientDir ?? "";
  clientFolderHelp.hidden = clientDir !== null;
}

function showMessage(message: string, kind: "success" | "error" | "info"): void {
  operationMessage.textContent = message;
  operationMessage.dataset.kind = kind;
  operationMessage.setAttribute("role", kind === "error" ? "alert" : "status");
  operationMessage.hidden = false;
}

function clearMessage(): void {
  operationMessage.textContent = "";
  operationMessage.hidden = true;
}

function errorMessage(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return "Se produjo un error inesperado.";
}

async function checkClient(): Promise<void> {
  if (!clientDir || isChecking || isUpdating || isLaunching || isClearingCache) return;
  isChecking = true;
  refreshButtons();
  clearMessage();

  try {
    clientFiles = await invoke<ClientFileStatus[]>("check_client_status");
    hasCheckedClient = true;
    renderFiles();
    const attentionCount = clientFiles.filter((file) => file.status !== "ok").length;
    showMessage(
      attentionCount === 0
        ? "Todos los archivos comprobados están en buen estado."
        : `Comprobación completada: ${attentionCount} archivo${attentionCount === 1 ? " necesita" : "s necesitan"} atención.`,
      attentionCount === 0 ? "success" : "info",
    );
  } catch (error: unknown) {
    showMessage(`No se pudo comprobar el cliente: ${errorMessage(error)}`, "error");
  } finally {
    isChecking = false;
    refreshButtons();
  }
}

async function updateClient(): Promise<void> {
  if (!clientDir || isChecking || isUpdating || isLaunching || isClearingCache) return;
  isUpdating = true;
  refreshButtons();
  clearMessage();
  progressSection.hidden = false;
  progressTitle.textContent = "Actualizando cliente";
  progressCount.textContent = "Preparando…";
  progressDetail.textContent = "Preparando la actualización…";
  progressBar.max = 1;
  progressBar.value = 0;

  let stopListening: (() => void) | undefined;
  try {
    stopListening = await listen<UpdateProgressPayload>("update-progress", ({ payload }) => {
      progressBar.max = Math.max(payload.total, 1);
      progressBar.value = Math.min(payload.index, progressBar.max);
      progressCount.textContent = `${payload.index} de ${payload.total}`;
      progressDetail.textContent = payload.message
        ? `${payload.path}: ${payload.message}`
        : `${payload.path} · ${payload.status === "ok" ? "completado" : "error"}`;
      if (payload.status === "error") {
        progressDetail.dataset.kind = "error";
      } else {
        delete progressDetail.dataset.kind;
      }
      updateDisplayedFile(payload);
    });

    await invoke<void>("update_client");
    progressTitle.textContent = "Actualización completada";
    progressCount.textContent = "Completado";
    progressDetail.textContent = "Los archivos obligatorios están preparados.";
    progressBar.value = progressBar.max;
    showMessage("Actualización completada.", "success");
  } catch (error: unknown) {
    const message = errorMessage(error);
    progressTitle.textContent = "Actualización interrumpida";
    progressCount.textContent = "No completada";
    progressDetail.textContent = message;
    progressDetail.dataset.kind = "error";
    showMessage(`No se pudo completar la actualización: ${message}`, "error");
  } finally {
    stopListening?.();
    isUpdating = false;
    refreshButtons();
  }
}

async function launchGame(): Promise<void> {
  if (!clientDir || !hasCheckedClient || isChecking || isUpdating || isLaunching || isClearingCache) return;
  isLaunching = true;
  refreshButtons();
  clearMessage();

  try {
    await invoke<void>("launch_game");
    showMessage("El juego se ha iniciado.", "success");
  } catch (error: unknown) {
    showMessage(`No se pudo iniciar el juego: ${errorMessage(error)}`, "error");
  } finally {
    isLaunching = false;
    refreshButtons();
  }
}

async function clearCache(): Promise<void> {
  if (!clientDir || isChecking || isUpdating || isLaunching || isClearingCache) return;

  try {
    const confirmed = await ask(
      "Se borrará la carpeta Cache del cliente. Cierra el juego antes de continuar. ¿Continuar?",
      { title: "Borrar caché", kind: "warning" },
    );
    if (!confirmed) return;

    isClearingCache = true;
    refreshButtons();
    clearMessage();
    await invoke<void>("clear_cache");
    showMessage("La caché del cliente se ha borrado.", "success");
  } catch (error: unknown) {
    showMessage(`No se pudo borrar la caché: ${errorMessage(error)}`, "error");
  } finally {
    isClearingCache = false;
    refreshButtons();
  }
}

function renderFiles(): void {
  fileList.replaceChildren();

  if (clientFiles.length === 0) {
    fileList.append(createEmptyState("No se encontraron archivos en el manifest."));
    fileCount.textContent = "0 archivos";
    return;
  }

  const attentionCount = clientFiles.filter((file) => file.status !== "ok").length;
  fileCount.textContent = attentionCount === 0
    ? `${clientFiles.length} archivos correctos`
    : `${attentionCount} requieren atención`;

  for (const file of clientFiles) {
    fileList.append(createFileRow(file));
  }
}

function createFileRow(file: ClientFileStatus): HTMLLIElement {
  const row = document.createElement("li");
  row.className = "file-row";
  row.dataset.status = file.status;

  const fileDetails = document.createElement("div");
  fileDetails.className = "file-details";
  const path = document.createElement("span");
  path.className = "file-path";
  path.textContent = file.path;
  const role = document.createElement("span");
  role.className = "file-role";
  role.textContent = file.role === "required" ? "Obligatorio" : "Opcional";
  fileDetails.append(path, role);

  const status = document.createElement("span");
  status.className = "file-status";
  status.textContent = statusLabel(file.status);

  row.append(fileDetails, status);
  if (file.message) {
    const message = document.createElement("span");
    message.className = "file-message";
    message.textContent = file.message;
    row.append(message);
  }
  return row;
}

function createEmptyState(message: string): HTMLLIElement {
  const item = document.createElement("li");
  item.className = "empty-state";
  item.textContent = message;
  return item;
}

function statusLabel(status: ClientFileState): string {
  switch (status) {
    case "ok": return "Correcto";
    case "missing": return "Falta";
    case "corrupt": return "Modificado";
    case "error": return "Error";
  }
}

function updateDisplayedFile(progress: UpdateProgressPayload): void {
  const index = clientFiles.findIndex((file) => file.path === progress.path);
  if (index < 0) return;
  clientFiles[index] = {
    ...clientFiles[index],
    status: progress.status === "ok" ? "ok" : "error",
    message: progress.message,
  };
  renderFiles();
}
