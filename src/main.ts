import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ask, open } from "@tauri-apps/plugin-dialog";

type ClientFileState = "ok" | "missing" | "corrupt" | "error";
type FileRole = "required" | "optional";
type UpdateFileState = "ok" | "error";
type MessageKind = "success" | "error" | "warning" | "info";

interface ClientFileStatus {
  path: string;
  role: FileRole;
  status: ClientFileState;
  message: string | null;
}

interface ClientStatusResponse {
  files: ClientFileStatus[];
  installed: boolean;
}

interface UpdateProgressPayload {
  path: string;
  index: number;
  total: number;
  status: UpdateFileState;
  message: string | null;
}

interface VerifyProgressPayload {
  index: number;
  total: number;
  path: string;
  fileBytesDone: number;
  fileBytesTotal: number;
  bytesDone: number;
  bytesTotal: number;
}

interface LauncherSettings {
  clientDir: string | null;
}

const logo = requiredElement<HTMLImageElement>("#brand-logo");
const checkButton = requiredElement<HTMLButtonElement>("#check-button");
const updateButton = requiredElement<HTMLButtonElement>("#update-button");
const playButton = requiredElement<HTMLButtonElement>("#play-button");
const clearCacheButton = requiredElement<HTMLButtonElement>("#clear-cache-button");
const chooseFolderButton = requiredElement<HTMLButtonElement>("#choose-folder-button");
const newInstallButton = requiredElement<HTMLButtonElement>("#new-install-button");
const cancelButton = requiredElement<HTMLButtonElement>("#cancel-button");
const clientLocation = requiredElement<HTMLParagraphElement>("#client-location");
const clientFolderHelp = requiredElement<HTMLParagraphElement>("#client-folder-help");
const operationMessage = requiredElement<HTMLDivElement>("#operation-message");
const progressMeter = requiredElement<HTMLDivElement>("#progress-meter");
const progressBar = requiredElement<HTMLProgressElement>("#update-progress");
const progressStatus = requiredElement<HTMLParagraphElement>("#progress-status");
const clientSummary = requiredElement<HTMLDivElement>("#client-summary");
const statusIcon = requiredElement<HTMLSpanElement>("#status-icon");
const statusText = requiredElement<HTMLSpanElement>("#status-text");
const summaryDescription = requiredElement<HTMLSpanElement>("#summary-description");
const detailsToggle = requiredElement<HTMLButtonElement>("#details-toggle");
const detailsLabel = requiredElement<HTMLSpanElement>("#details-label");
const fileSection = requiredElement<HTMLElement>("#file-section");
const fileList = requiredElement<HTMLUListElement>("#file-list");
const fileCount = requiredElement<HTMLSpanElement>("#file-count");
const showHealthyFiles = requiredElement<HTMLInputElement>("#show-healthy-files");
const logoUrl = new URL("./assets/logo-warcrafted.jpg", import.meta.url).href;

let clientFiles: ClientFileStatus[] = [];
let clientDir: string | null = null;
let clientInstalled: boolean | null = null;
let hasCheckedClient = false;
let isChecking = false;
let isUpdating = false;
let isLaunching = false;
let isClearingCache = false;
let isPreparingInstall = false;
let isCanceling = false;
let isLoadingSettings = true;

logo.src = logoUrl;
chooseFolderButton.addEventListener("click", () => void chooseClientFolder());
newInstallButton.addEventListener("click", () => void createNewInstall());
checkButton.addEventListener("click", () => void checkClient());
updateButton.addEventListener("click", () => void updateClient());
playButton.addEventListener("click", () => void launchGame());
clearCacheButton.addEventListener("click", () => void clearCache());
detailsToggle.addEventListener("click", toggleFileDetails);
cancelButton.addEventListener("click", () => void cancelCurrentOperation());
showHealthyFiles.addEventListener("change", () => renderFiles());

void loadSettings();

function requiredElement<T extends HTMLElement>(selector: string): T {
  const element = document.querySelector<T>(selector);
  if (!element) {
    throw new Error(`No se encontró el elemento de interfaz ${selector}.`);
  }
  return element;
}

function isBusy(): boolean {
  return isLoadingSettings || isChecking || isUpdating || isLaunching || isClearingCache || isPreparingInstall;
}

function refreshButtons(): void {
  const busy = isBusy();
  const canCancel = isChecking || isUpdating || isLaunching;
  const hasClientDirectory = clientDir !== null;
  document.body.classList.toggle("is-busy", busy && !isLoadingSettings);
  chooseFolderButton.disabled = busy;
  newInstallButton.disabled = busy;
  checkButton.disabled = busy || !hasClientDirectory;
  updateButton.disabled = busy || !hasClientDirectory;
  clearCacheButton.disabled = busy || !hasClientDirectory;
  playButton.disabled = busy || !hasClientDirectory || !hasCheckedClient || clientInstalled === false;
  const actionTitle = busy
    ? "Espera a que termine la operación actual"
    : !hasClientDirectory
      ? "Elige primero la carpeta del cliente"
      : "";
  checkButton.title = actionTitle;
  updateButton.title = actionTitle;
  clearCacheButton.title = actionTitle;
  playButton.title = busy
    ? "Espera a que termine la operación actual"
    : !hasClientDirectory
      ? "Elige primero la carpeta del cliente"
      : clientInstalled === false
        ? "Instala primero el cliente"
        : !hasCheckedClient
          ? "Comprueba el estado del cliente"
          : "Iniciar World of Warcraft";
  setButtonBusy(checkButton, isChecking, "Comprobando…", "Comprobar");
  setButtonBusy(
    updateButton,
    isUpdating,
    clientInstalled === false ? "Instalando…" : "Actualizando…",
    clientInstalled === false ? "Instalar" : "Actualizar",
  );
  setButtonBusy(clearCacheButton, isClearingCache, "Borrando…", "Borrar caché");
  setButtonBusy(playButton, isLaunching, "Iniciando…", "JUGAR");
  cancelButton.hidden = !canCancel;
  cancelButton.disabled = isCanceling;
  cancelButton.textContent = isCanceling ? "Cancelando…" : "Cancelar";
}

function setButtonBusy(button: HTMLButtonElement, busy: boolean, busyLabel: string, idleLabel: string): void {
  const spinner = button.querySelector<HTMLElement>(".button-spinner");
  const label = button.querySelector<HTMLElement>(".button-label");
  button.toggleAttribute("data-busy", busy);
  if (spinner) spinner.hidden = !busy;
  if (label) label.textContent = busy ? busyLabel : idleLabel;
  if (button === playButton) {
    const icon = button.querySelector<HTMLElement>(".play-icon");
    if (icon) icon.hidden = busy;
  }
}

async function loadSettings(): Promise<void> {
  try {
    const settings = await invoke<LauncherSettings>("get_settings");
    clientDir = settings.clientDir;
    clientInstalled = null;
    renderClientDirectory();
    setProgressStatus(clientDir
      ? "Carpeta cargada. Comprueba el estado del cliente."
      : "Elige la carpeta del cliente para comenzar.");
  } catch (error: unknown) {
    showMessage(`No se pudieron cargar los ajustes: ${errorMessage(error)}`, "error");
  } finally {
    isLoadingSettings = false;
    refreshButtons();
  }
}

async function chooseClientFolder(): Promise<void> {
  if (isBusy()) return;
  clearMessage();

  try {
    const selectedPath = await open({
      directory: true,
      multiple: false,
      title: "Elige la carpeta del cliente de WoW",
    });
    if (selectedPath === null) return;

    const savedPath = await invoke<string>("set_client_dir", { path: selectedPath });
    clientDir = savedPath;
    clientInstalled = null;
    hasCheckedClient = false;
    clientFiles = [];
    renderClientDirectory();
    renderFiles("Comprueba el estado para ver los archivos del cliente.");
    showHealthyFiles.checked = false;
    setSummary("Sin comprobar", "idle", "○");
    setProgressStatus("Carpeta guardada. Comprueba el estado del cliente.");
    hideProgressMeter();
    showMessage("La carpeta del cliente se ha guardado.", "success");
    refreshButtons();
  } catch (error: unknown) {
    showMessage(`No se pudo elegir la carpeta del cliente: ${errorMessage(error)}`, "error");
  }
}

async function createNewInstall(): Promise<void> {
  if (isBusy()) return;

  isPreparingInstall = true;
  refreshButtons();
  clearMessage();
  let shouldCheck = false;
  try {
    const parent = await open({
      directory: true,
      multiple: false,
      title: "Elige dónde instalar WarCrafted",
    });
    if (parent === null) return;

    const confirmed = await ask(
      `Se creará la carpeta «WarCrafted WotLK» en ${parent} y se descargará el cliente completo (unos 18,5 GB). ¿Continuar?`,
      { title: "Nueva instalación", kind: "info" },
    );
    if (!confirmed) return;

    const installPath = await invoke<string>("create_install_dir", { parent });
    clientDir = installPath;
    clientInstalled = false;
    hasCheckedClient = false;
    clientFiles = [];
    showHealthyFiles.checked = false;
    renderClientDirectory();
    renderFiles("Comprueba el estado para ver los archivos del cliente.");
    setSummary("Sin comprobar", "idle", "○");
    setProgressStatus("Carpeta de instalación preparada. Comprobando el cliente…");
    hideProgressMeter();
    showMessage(`Instalación preparada en ${installPath}.`, "success");
    shouldCheck = true;
  } catch (error: unknown) {
    showMessage(`No se pudo preparar la instalación: ${errorMessage(error)}`, "error");
  } finally {
    isPreparingInstall = false;
    refreshButtons();
  }

  if (shouldCheck) await checkClient();
}

function renderClientDirectory(): void {
  clientLocation.textContent = clientDir ?? "Ninguna carpeta elegida";
  clientLocation.title = clientDir ?? "";
  clientLocation.classList.toggle("is-empty", clientDir === null);
  chooseFolderButton.textContent = clientDir ? "Cambiar…" : "Elegir carpeta…";
  clientFolderHelp.hidden = clientDir !== null;
}

function showMessage(message: string, kind: MessageKind): void {
  operationMessage.textContent = message;
  operationMessage.dataset.kind = kind;
  operationMessage.setAttribute("role", kind === "error" ? "alert" : "status");
  operationMessage.hidden = false;
}

function clearMessage(): void {
  operationMessage.textContent = "";
  operationMessage.hidden = true;
  delete operationMessage.dataset.kind;
}

function errorMessage(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return "Se produjo un error inesperado.";
}

async function checkClient(): Promise<void> {
  if (!clientDir || isBusy()) return;
  const previousSummary = {
    text: statusText.textContent ?? "",
    description: summaryDescription.textContent ?? "",
    state: clientSummary.dataset.state ?? "idle",
    icon: statusIcon.textContent ?? "○",
  };
  isChecking = true;
  refreshButtons();
  clearMessage();
  setSummary("Comprobando archivos del cliente…", "busy", "◌");
  setProgressStatus("Preparando la comprobación…");
  hideProgressMeter();

  let stopListening: (() => void) | undefined;
  try {
    stopListening = await listen<VerifyProgressPayload>("verify-progress", ({ payload }) => {
      showVerifyProgress(payload);
    });
    const status = await invoke<ClientStatusResponse>("check_client_status");
    clientFiles = status.files;
    clientInstalled = status.installed;
    hasCheckedClient = true;
    const attentionCount = clientFiles.filter((file) => file.status !== "ok").length;
    showHealthyFiles.checked = attentionCount === 0;
    renderFiles();
    if (!status.installed) {
      setSummary("Cliente no instalado", "warning", "!", "Se descargará el cliente completo (unos 18,5 GB)");
      setProgressStatus("El cliente no está instalado. Puedes descargarlo con el botón Instalar.");
      showMessage("El cliente no está instalado. Se descargará completo (unos 18,5 GB).", "info");
    } else {
      const summary = attentionCount === 0
        ? "Cliente listo para jugar"
        : `${attentionCount} ${attentionCount === 1 ? "archivo requiere" : "archivos requieren"} atención`;
      setSummary(summary, attentionCount === 0 ? "ok" : "warning", attentionCount === 0 ? "✓" : "!");
      setProgressStatus(attentionCount === 0
        ? "Comprobación completada. Todos los archivos están en buen estado."
        : `Comprobación completada. ${summary}.`);
      showMessage(
        attentionCount === 0
          ? "Todos los archivos obligatorios están en buen estado."
          : `Comprobación completada: ${summary}.`,
        attentionCount === 0 ? "success" : "warning",
      );
    }
  } catch (error: unknown) {
    const message = errorMessage(error);
    if (isCancellation(message)) {
      restoreSummary(previousSummary);
      setProgressStatus("Operación cancelada.");
      showMessage("Operación cancelada.", "info");
    } else {
      setSummary("No se pudo comprobar el cliente", "error", "!");
      setProgressStatus("La comprobación no pudo completarse.");
      showMessage(`No se pudo comprobar el cliente: ${message}`, "error");
    }
  } finally {
    stopListening?.();
    hideProgressMeter();
    isChecking = false;
    isCanceling = false;
    refreshButtons();
  }
}

async function updateClient(): Promise<void> {
  if (!clientDir || isBusy()) return;
  isUpdating = true;
  refreshButtons();
  clearMessage();
  setProgressStatus("Preparando la actualización…");
  hideProgressMeter();

  let stopVerifyListening: (() => void) | undefined;
  let stopUpdateListening: (() => void) | undefined;
  try {
    stopVerifyListening = await listen<VerifyProgressPayload>("verify-progress", ({ payload }) => {
      showVerifyProgress(payload);
    });
    stopUpdateListening = await listen<UpdateProgressPayload>("update-progress", ({ payload }) => {
      showUpdateProgress(payload);
      updateDisplayedFile(payload);
    });

    await invoke<void>("update_client");
    const status = await invoke<ClientStatusResponse>("check_client_status");
    clientFiles = status.files;
    clientInstalled = status.installed;
    hasCheckedClient = true;
    const attentionCount = clientFiles.filter((file) => file.status !== "ok").length;
    showHealthyFiles.checked = attentionCount === 0;
    renderFiles();
    if (!status.installed) {
      setSummary("Cliente no instalado", "warning", "!", "Se descargará el cliente completo (unos 18,5 GB)");
    } else if (attentionCount === 0) {
      setSummary("Cliente listo para jugar", "ok", "✓");
    } else {
      setSummary(`${attentionCount} ${attentionCount === 1 ? "archivo requiere" : "archivos requieren"} atención`, "warning", "!");
    }
    setProgressStatus(status.installed
      ? "Actualización completada. Los archivos obligatorios están preparados."
      : "La instalación no está completa. Comprueba los archivos y vuelve a intentarlo.");
    showMessage("Actualización completada.", "success");
  } catch (error: unknown) {
    const message = errorMessage(error);
    if (isCancellation(message)) {
      setProgressStatus("Operación cancelada.");
      showMessage("Operación cancelada.", "info");
    } else {
      setProgressStatus("La actualización no pudo completarse.");
      showMessage(`No se pudo completar la actualización: ${message}`, "error");
    }
  } finally {
    stopUpdateListening?.();
    stopVerifyListening?.();
    hideProgressMeter();
    isUpdating = false;
    isCanceling = false;
    refreshButtons();
  }
}

async function launchGame(): Promise<void> {
  if (!clientDir || !hasCheckedClient || isBusy()) return;
  isLaunching = true;
  refreshButtons();
  clearMessage();
  setProgressStatus("Validando los archivos antes de iniciar…");
  hideProgressMeter();

  let stopListening: (() => void) | undefined;
  try {
    stopListening = await listen<VerifyProgressPayload>("verify-progress", ({ payload }) => {
      showVerifyProgress(payload);
    });
    await invoke<void>("launch_game");
    setProgressStatus("El juego se ha iniciado.");
    showMessage("El juego se ha iniciado.", "success");
  } catch (error: unknown) {
    const message = errorMessage(error);
    if (isCancellation(message)) {
      setProgressStatus("Operación cancelada.");
      showMessage("Operación cancelada.", "info");
    } else {
      setProgressStatus("No se pudo iniciar el juego.");
      showMessage(`No se pudo iniciar el juego: ${message}`, "error");
    }
  } finally {
    stopListening?.();
    hideProgressMeter();
    isLaunching = false;
    isCanceling = false;
    refreshButtons();
  }
}

async function clearCache(): Promise<void> {
  if (!clientDir || isBusy()) return;

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
    setProgressStatus("Caché del cliente borrada.");
    showMessage("La caché del cliente se ha borrado.", "success");
  } catch (error: unknown) {
    showMessage(`No se pudo borrar la caché: ${errorMessage(error)}`, "error");
  } finally {
    isClearingCache = false;
    refreshButtons();
  }
}

function showVerifyProgress(payload: VerifyProgressPayload): void {
  const percentage = payload.fileBytesTotal > 0
    ? Math.min(100, Math.floor((payload.fileBytesDone / payload.fileBytesTotal) * 100))
    : 0;
  const path = fileName(payload.path);
  setProgressStatus(`Verificando archivo ${payload.index} de ${payload.total} · ${path} (${percentage} %)`);
  progressBar.max = Math.max(payload.bytesTotal, 1);
  progressBar.value = Math.min(Math.max(payload.bytesDone, 0), progressBar.max);
  progressMeter.hidden = false;
}

function showUpdateProgress(payload: UpdateProgressPayload): void {
  const total = Math.max(payload.total, 1);
  setProgressStatus(`Descargando ${payload.index} de ${payload.total} · ${fileName(payload.path)}`);
  if (payload.status === "error") {
    progressStatus.dataset.kind = "error";
  }
  progressBar.max = total;
  progressBar.value = Math.min(Math.max(payload.index, 0), total);
  progressMeter.hidden = false;
}

function setProgressStatus(message: string): void {
  progressStatus.textContent = message;
  delete progressStatus.dataset.kind;
}

async function cancelCurrentOperation(): Promise<void> {
  if (isCanceling || !(isChecking || isUpdating || isLaunching)) return;

  isCanceling = true;
  refreshButtons();
  try {
    await invoke<void>("cancel_operation");
  } catch (error: unknown) {
    isCanceling = false;
    showMessage(`No se pudo cancelar la operación: ${errorMessage(error)}`, "error");
    refreshButtons();
  }
}

function isCancellation(message: string): boolean {
  return message === "Operación cancelada.";
}

function restoreSummary(summary: { text: string; description: string; state: string; icon: string }): void {
  statusText.textContent = summary.text;
  summaryDescription.textContent = summary.description;
  statusIcon.textContent = summary.icon;
  clientSummary.dataset.state = summary.state;
}

function hideProgressMeter(): void {
  progressMeter.hidden = true;
}

function fileName(path: string): string {
  return path.split(/[\\/]/).pop() || path;
}

function setSummary(
  message: string,
  state: "idle" | "busy" | "ok" | "warning" | "error",
  icon: string,
  description = "",
): void {
  statusText.textContent = message;
  summaryDescription.textContent = description;
  statusIcon.textContent = icon;
  clientSummary.dataset.state = state;
}

function toggleFileDetails(): void {
  const expanded = detailsToggle.getAttribute("aria-expanded") === "true";
  detailsToggle.setAttribute("aria-expanded", String(!expanded));
  detailsLabel.textContent = expanded ? "Ver detalles" : "Ocultar detalles";
  fileSection.hidden = expanded;
}

function renderFiles(emptyMessage = "No se encontraron archivos en el manifest."): void {
  fileList.replaceChildren();

  if (clientFiles.length === 0) {
    fileList.append(createEmptyState(emptyMessage));
    fileCount.textContent = "0 archivos";
    return;
  }

  const attentionCount = clientFiles.filter((file) => file.status !== "ok").length;
  const visibleFiles = [...clientFiles]
    .sort((left, right) => statusPriority(left.status) - statusPriority(right.status))
    .filter((file) => showHealthyFiles.checked || file.status !== "ok");
  fileCount.textContent = attentionCount === 0
    ? `${clientFiles.length} archivos correctos`
    : `${attentionCount} requieren atención · ${visibleFiles.length} visibles`;

  for (const file of visibleFiles) {
    fileList.append(createFileRow(file));
  }
}

function createFileRow(file: ClientFileStatus): HTMLLIElement {
  const row = document.createElement("li");
  row.className = "file-row";
  row.dataset.status = file.status;

  const fileDetails = document.createElement("div");
  fileDetails.className = "file-details";
  const name = document.createElement("span");
  name.className = "file-name";
  name.textContent = fileName(file.path);
  const path = document.createElement("span");
  path.className = "file-path";
  path.textContent = file.path;
  path.title = file.path;
  const role = document.createElement("span");
  role.className = "file-role";
  role.textContent = file.role === "required" ? "Obligatorio" : "Opcional";
  fileDetails.append(name, path, role);

  const status = document.createElement("span");
  status.className = "file-status";
  const statusIcon = document.createElement("span");
  statusIcon.className = "file-status-icon";
  statusIcon.setAttribute("aria-hidden", "true");
  statusIcon.textContent = fileStatusIcon(file.status);
  const statusLabelText = document.createElement("span");
  statusLabelText.textContent = statusLabel(file.status);
  status.append(statusIcon, statusLabelText);

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

function fileStatusIcon(status: ClientFileState): string {
  switch (status) {
    case "ok": return "✓";
    case "missing": return "!";
    case "corrupt": return "↻";
    case "error": return "×";
  }
}

function statusPriority(status: ClientFileState): number {
  switch (status) {
    case "error": return 0;
    case "corrupt": return 1;
    case "missing": return 2;
    case "ok": return 3;
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
  if (clientFiles.every((file) => file.status === "ok")) showHealthyFiles.checked = true;
  renderFiles();
}
