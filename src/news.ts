import sampleContent from "./content/content.sample.json";

export type NewsCategory = "parche" | "evento" | "mantenimiento" | "anuncio";
export type HighlightIcon = "runes" | "swords" | "shield" | "scroll";
export type NewsBlock =
  | { type: "heading"; text: string }
  | { type: "paragraph"; text: string }
  | { type: "list"; items: string[] };

export interface NewsArticle {
  id: string;
  title: string;
  summary: string;
  date: string;
  category: NewsCategory;
  featured: boolean;
  image: string | null;
  link: string | null;
  body: NewsBlock[];
}

export interface Highlight {
  icon: HighlightIcon;
  title: string;
  text: string;
}

export interface OfficialLink {
  label: string;
  url: string;
  description: string;
}

export interface NewsContent {
  schemaVersion: 1;
  news: NewsArticle[];
  highlights: Highlight[];
  links: OfficialLink[];
}

export interface DetectedClient {
  path: string;
  version: string | null;
  valid: boolean;
}

export interface ContentActions {
  chooseFolder(): void;
  newInstall(): void;
  fullCheck(): void;
  clearCache(): void;
  openExternal(url: string): void;
  detectClients(): Promise<DetectedClient[]>;
  useClientFolder(path: string): Promise<void>;
  cancelDetection(): void;
}

export interface ContentClientState {
  hasClientDirectory: boolean;
  checked: boolean;
  mandatoryAddons: Array<{ name: string; status: string; state: string }>;
}

const categories: NewsCategory[] = ["parche", "evento", "mantenimiento", "anuncio"];
const icons: HighlightIcon[] = ["runes", "swords", "shield", "scroll"];
const categoryNames: Record<NewsCategory, string> = {
  parche: "Parche",
  evento: "Evento",
  mantenimiento: "Mantenimiento",
  anuncio: "Anuncio",
};

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isText(value: unknown): value is string {
  return typeof value === "string" && value.trim().length > 0;
}

function isHttpsUrl(value: unknown): value is string {
  if (typeof value !== "string") return false;
  try {
    return new URL(value).protocol === "https:";
  } catch {
    return false;
  }
}

function isDate(value: unknown): value is string {
  if (typeof value !== "string" || !/^\d{4}-\d{2}-\d{2}$/.test(value)) return false;
  const parsed = new Date(`${value}T12:00:00Z`);
  return Number.isFinite(parsed.getTime()) && parsed.toISOString().slice(0, 10) === value;
}

function parseBlock(value: unknown): NewsBlock | null {
  if (!isRecord(value)) return null;
  if ((value.type === "heading" || value.type === "paragraph") && isText(value.text)) {
    return { type: value.type, text: value.text };
  }
  if (value.type === "list" && Array.isArray(value.items) && value.items.every(isText)) {
    return { type: "list", items: value.items };
  }
  return null;
}

function parseArticle(value: unknown): NewsArticle | null {
  if (!isRecord(value)
    || !isText(value.id)
    || !isText(value.title)
    || !isText(value.summary)
    || !isDate(value.date)
    || typeof value.category !== "string"
    || !categories.includes(value.category as NewsCategory)
    || typeof value.featured !== "boolean"
    || !(value.image === null || isHttpsUrl(value.image))
    || !(value.link === null || isHttpsUrl(value.link))
    || !Array.isArray(value.body)) return null;
  const body = value.body.map(parseBlock);
  if (body.some((block) => block === null)) return null;
  return {
    id: value.id,
    title: value.title,
    summary: value.summary,
    date: value.date,
    category: value.category as NewsCategory,
    featured: value.featured,
    image: value.image,
    link: value.link,
    body: body as NewsBlock[],
  };
}

export function validateNewsContent(input: unknown): NewsContent {
  if (!isRecord(input) || input.schemaVersion !== 1) {
    return { schemaVersion: 1, news: [], highlights: [], links: [] };
  }

  const news = Array.isArray(input.news)
    ? input.news.map(parseArticle).filter((article): article is NewsArticle => article !== null)
    : [];
  const highlights = Array.isArray(input.highlights)
    ? input.highlights.flatMap((entry): Highlight[] => {
      if (!isRecord(entry) || typeof entry.icon !== "string" || !icons.includes(entry.icon as HighlightIcon)
        || !isText(entry.title) || !isText(entry.text)) return [];
      return [{ icon: entry.icon as HighlightIcon, title: entry.title, text: entry.text }];
    })
    : [];
  const links = Array.isArray(input.links)
    ? input.links.flatMap((entry): OfficialLink[] => {
      if (!isRecord(entry) || !isText(entry.label) || !isHttpsUrl(entry.url) || !isText(entry.description)) return [];
      return [{ label: entry.label, url: entry.url, description: entry.description }];
    })
    : [];
  return { schemaVersion: 1, news, highlights, links };
}

export const content = validateNewsContent(sampleContent);

export function initializeContentUI(actions: ContentActions): void {
  const panels = requiredElement<HTMLDivElement>("#tab-panels");
  panels.replaceChildren(createHomePanel(actions), createNewsPanel(actions), createAddonsPanel(), createSettingsPanel(actions));
  setupTabs();
  setupFeaturedRotation();
}

export function updateContentClientState(state: ContentClientState): void {
  requiredElement<HTMLElement>("#setup-wizard").hidden = state.hasClientDirectory;
  requiredElement<HTMLElement>("#home-dashboard").hidden = !state.hasClientDirectory;

  const list = requiredElement<HTMLUListElement>("#mandatory-addon-list");
  list.replaceChildren();
  const message = requiredElement<HTMLParagraphElement>("#addon-empty-state");
  message.hidden = state.checked;
  if (!state.checked) return;
  if (state.mandatoryAddons.length === 0) {
    list.append(emptyListItem("No se encontraron addons obligatorios en la última comprobación."));
    return;
  }
  for (const addon of state.mandatoryAddons) {
    const row = document.createElement("li");
    row.className = "addon-row";
    const details = document.createElement("div");
    details.className = "addon-details";
    const title = document.createElement("strong");
    title.textContent = addon.name;
    const managed = document.createElement("span");
    managed.className = "managed-label";
    managed.textContent = "Gestionado automáticamente";
    details.append(title, managed);
    const status = document.createElement("span");
    status.className = "addon-status";
    status.dataset.state = addon.state;
    status.textContent = addon.status;
    row.append(details, status);
    list.append(row);
  }
}

function createHomePanel(actions: ContentActions): HTMLElement {
  const panel = createPanel("home", true);
  const wizard = element("section", "setup-wizard panel");
  wizard.id = "setup-wizard";
  wizard.setAttribute("aria-labelledby", "setup-title");
  const wizardCopy = element("div", "wizard-copy");
  wizardCopy.append(eyebrow("EMPIEZA TU AVENTURA"));
  const wizardTitle = element("h1", "wizard-title", "Prepara WarCrafted");
  wizardTitle.id = "setup-title";
  wizardCopy.append(wizardTitle, paragraph("Elige cómo quieres preparar tu cliente de World of Warcraft 3.3.5a."));
  const options = element("div", "wizard-options");
  const install = button("button button-primary", "Instalar WarCrafted", actions.newInstall);
  install.append(createThemedIcon("download"));
  const existing = button("button button-secondary", "Ya tengo el cliente", actions.chooseFolder);
  existing.append(createThemedIcon("folder"));
  options.append(install, existing);
  const wizardSearch = createClientSearch(actions);
  wizardSearch.classList.add("wizard-search");
  wizard.append(wizardCopy, options, wizardSearch);

  const dashboard = element("div", "home-dashboard");
  dashboard.id = "home-dashboard";
  dashboard.hidden = true;
  const top = element("div", "home-top-grid");
  const carousel = element("section", "featured-carousel panel");
  carousel.setAttribute("aria-label", "Noticias destacadas");
  carousel.id = "featured-carousel";
  renderFeatured(carousel, 0);
  const latest = element("section", "latest-news panel");
  const latestHead = element("div", "section-heading");
  latestHead.append(heading("h2", "Últimas noticias"));
  latestHead.append(button("text-button", "Ver todas →", () => activateTab("news")));
  latest.append(latestHead);
  const latestList = element("div", "latest-list");
  const latestArticles = sortedNews().slice(0, 5);
  for (const article of latestArticles) latestList.append(createNewsIndexItem(article, () => openArticle(article.id)));
  if (latestArticles.length === 0) latestList.append(emptyMessage("Pronto habrá noticias del reino."));
  latest.append(latestList);
  top.append(carousel, latest);

  const lower = element("div", "home-lower-grid");
  const highlights = element("section", "highlights-section");
  const highlightsTitle = heading("h2", "El reino, a tu manera");
  const highlightGrid = element("div", "highlight-grid");
  for (const item of content.highlights) {
    const card = element("article", "highlight-card panel");
    card.append(createThemedIcon(item.icon));
    const copy = element("div", "highlight-copy");
    copy.append(heading("h3", item.title), paragraph(item.text));
    card.append(copy);
    highlightGrid.append(card);
  }
  highlights.append(highlightsTitle, highlightGrid);

  const linksCard = element("section", "official-links panel");
  linksCard.append(eyebrow("SERVICIOS OFICIALES"), heading("h2", "WarCrafted"));
  const linkList = element("div", "official-link-list");
  for (const link of content.links) {
    const anchor = document.createElement("button");
    anchor.type = "button";
    anchor.className = "official-link";
    anchor.addEventListener("click", () => actions.openExternal(link.url));
    const label = document.createElement("strong");
    label.textContent = link.label;
    const description = document.createElement("span");
    description.textContent = link.description;
    anchor.append(label, description, createThemedIcon("external"));
    linkList.append(anchor);
  }
  linksCard.append(linkList);
  lower.append(highlights, linksCard);
  dashboard.append(top, lower);
  panel.append(wizard, dashboard);
  return panel;
}

function createNewsPanel(actions: ContentActions): HTMLElement {
  const panel = createPanel("news");
  const layout = element("div", "news-layout");
  const index = element("aside", "news-index panel");
  index.setAttribute("aria-label", "Índice de noticias");
  index.append(heading("h2", "Noticias"));
  const filters = element("div", "category-filters");
  const categoryValues: Array<NewsCategory | "todas"> = ["todas", ...categories];
  for (const category of categoryValues) {
    const chip = button("category-chip", category === "todas" ? "Todas" : categoryNames[category], () => {
      selectedCategory = category;
      selectedNewsId = currentArticle?.id ?? sortedNews()[0]?.id ?? null;
      renderNewsIndex(index);
      renderArticle(reader, actions);
    });
    chip.dataset.category = category;
    chip.setAttribute("aria-pressed", String(category === selectedCategory));
    filters.append(chip);
  }
  index.append(filters);
  const list = element("div", "news-full-list");
  list.id = "news-full-list";
  index.append(list);
  const reader = element("article", "news-reader panel");
  reader.id = "news-reader";
  layout.append(index, reader);
  panel.append(layout);
  renderNewsIndex(index);
  renderArticle(reader, actions);
  return panel;
}

function createAddonsPanel(): HTMLElement {
  const panel = createPanel("addons");
  const addons = element("div", "addons-page");
  addons.append(eyebrow("CONTENIDO DEL JUEGO"), heading("h1", "Addons"));
  const sections = element("div", "addon-sections");
  const mandatory = element("section", "addon-section mandatory-section panel");
  mandatory.append(eyebrow("REQUERIDOS PARA JUGAR"), heading("h2", "Obligatorios"));
  const list = element("ul", "mandatory-addon-list");
  list.id = "mandatory-addon-list";
  list.append(emptyListItem("Comprueba el cliente para ver los addons obligatorios."));
  const empty = paragraph("Comprueba el cliente para ver los addons obligatorios.", "addon-empty-state");
  empty.id = "addon-empty-state";
  mandatory.append(list, empty);

  const optional = element("section", "addon-section optional-section panel");
  optional.append(eyebrow("A TU ELECCIÓN"), heading("h2", "Opcionales"));
  optional.append(paragraph("Próximamente podrás instalar addons opcionales desde aquí.", "optional-empty-state"));
  sections.append(mandatory, optional);
  addons.append(sections);
  panel.append(addons);
  return panel;
}

function createSettingsPanel(actions: ContentActions): HTMLElement {
  const panel = createPanel("settings");
  const settings = element("div", "settings-page");
  settings.append(eyebrow("TU CONFIGURACIÓN"), heading("h1", "Ajustes"));
  const folder = element("section", "settings-card folder-card panel");
  const copy = element("div", "folder-copy");
  copy.append(eyebrow("PREPARACIÓN DEL JUEGO"), heading("h2", "Carpeta del cliente"));
  const location = paragraph("Ninguna carpeta elegida", "client-location is-empty");
  location.id = "client-location";
  const help = paragraph("El cliente completo ocupa aproximadamente 18,5 GB.", "client-folder-help");
  help.id = "client-folder-help";
  const diskSpace = paragraph("", "client-folder-help client-disk-space");
  diskSpace.id = "client-disk-space";
  diskSpace.hidden = true;
  copy.append(location, help, diskSpace);
  const folderActions = element("div", "folder-actions");
  folderActions.append(
    button("button button-secondary", "Elegir carpeta…", actions.chooseFolder, "choose-folder-button"),
    button("button button-secondary", "Nueva instalación…", actions.newInstall, "new-install-button"),
  );
  folder.append(copy, folderActions);

  const maintenance = element("section", "maintenance-card panel");
  const maintenanceCopy = element("div", "maintenance-copy");
  maintenanceCopy.append(heading("h2", "Mantenimiento"), paragraph("Lee todos los archivos del cliente; puede tardar varios minutos.", "maintenance-help"));
  const maintenanceActions = element("div", "maintenance-actions");
  maintenanceActions.append(
    button("button button-secondary", "Verificación completa", actions.fullCheck, "full-check-button"),
    button("button button-secondary", "Borrar caché", actions.clearCache, "clear-cache-button"),
  );
  maintenance.append(maintenanceCopy, maintenanceActions);

  const summary = element("div", "settings-summary");
  summary.id = "settings-summary";
  summary.append(createThemedIcon("shield"), element("strong", "", "Estado del cliente: sin comprobar"));
  const files = element("section", "file-section panel");
  files.id = "file-section";
  files.setAttribute("aria-labelledby", "files-title");
  const fileHeading = element("div", "file-list-heading");
  fileHeading.append(heading("h2", "Archivos del cliente", "files-title"));
  const options = element("div", "file-list-options");
  const label = element("label", "show-healthy-option");
  label.htmlFor = "show-healthy-files";
  const checkbox = document.createElement("input");
  checkbox.type = "checkbox";
  checkbox.id = "show-healthy-files";
  label.append(checkbox, document.createTextNode("Mostrar también los correctos"));
  const count = element("span", "", "Aún no comprobados");
  count.id = "file-count";
  options.append(label, count);
  fileHeading.append(options);
  const fileList = element("ul", "file-list");
  fileList.id = "file-list";
  fileList.setAttribute("aria-live", "polite");
  fileList.append(emptyListItem("Comprueba el estado para ver los archivos del cliente."));
  files.append(fileHeading, fileList);
  settings.append(folder, createClientSearch(actions), maintenance, summary, files);
  panel.append(settings);
  return panel;
}

function renderFeatured(container: HTMLElement, index: number): void {
  const featured = sortedNews().filter((article) => article.featured);
  if (featured.length === 0) {
    container.replaceChildren(emptyMessage("Pronto habrá noticias destacadas."));
    return;
  }
  featuredIndex = index % featured.length;
  const article = featured[featuredIndex];
  const visual = element("div", "featured-visual");
  visual.dataset.category = article.category;
  if (article.image) {
    const image = document.createElement("img");
    image.src = article.image;
    image.alt = "";
    image.className = "featured-image";
    visual.append(image);
  } else {
    visual.append(createWatermarkLogo());
  }
  const overlay = element("div", "featured-overlay");
  const category = createCategoryBadge(article.category);
  const date = element("time", "news-date", formatDate(article.date));
  date.dateTime = article.date;
  const title = heading("h2", article.title);
  const summary = paragraph(article.summary);
  const more = button("button button-primary read-more-button", "Leer más", () => openArticle(article.id));
  const nav = element("div", "carousel-dots");
  nav.setAttribute("aria-label", "Seleccionar noticia destacada");
  featured.forEach((entry, dotIndex) => {
    const dot = button("carousel-dot", `Noticia ${dotIndex + 1}: ${entry.title}`, () => renderFeatured(container, dotIndex));
    dot.setAttribute("aria-label", `Mostrar noticia destacada ${dotIndex + 1}: ${entry.title}`);
    dot.setAttribute("aria-current", String(dotIndex === featuredIndex));
    nav.append(dot);
  });
  overlay.append(category, date, title, summary, more, nav);
  container.replaceChildren(visual, overlay);
}

function setupFeaturedRotation(): void {
  const featuredCount = content.news.filter((article) => article.featured).length;
  if (featuredCount < 2 || window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;
  const carousel = requiredElement<HTMLElement>("#featured-carousel");
  let paused = false;
  carousel.addEventListener("mouseenter", () => { paused = true; });
  carousel.addEventListener("mouseleave", () => { paused = false; });
  window.setInterval(() => {
    if (paused || document.hidden) return;
    renderFeatured(carousel, (featuredIndex + 1) % featuredCount);
  }, 8000);
}

function renderNewsIndex(index: HTMLElement): void {
  const list = index.querySelector<HTMLDivElement>(".news-full-list");
  if (!list) throw new Error("No se encontró la lista de noticias.");
  list.replaceChildren();
  const articles = sortedNews().filter((article) => selectedCategory === "todas" || article.category === selectedCategory);
  for (const article of articles) list.append(createNewsIndexItem(article, () => {
    selectedNewsId = article.id;
    index.querySelectorAll<HTMLElement>(".news-index-item").forEach((item) => {
      item.dataset.selected = String(item.querySelector("h3")?.textContent === article.title);
    });
    renderArticle(requiredElement<HTMLElement>("#news-reader"), { openExternal: (url) => openArticleUrl(url) });
  }, article.id === selectedNewsId));
  if (articles.length === 0) list.append(emptyMessage("No hay noticias en esta categoría."));
  index.querySelectorAll<HTMLButtonElement>(".category-chip").forEach((chip) => {
    chip.setAttribute("aria-pressed", String(chip.dataset.category === selectedCategory));
  });
}

function renderArticle(reader: HTMLElement, actions: Pick<ContentActions, "openExternal">): void {
  reader.replaceChildren();
  reader.scrollTop = 0;
  const article = content.news.find((entry) => entry.id === selectedNewsId)
    ?? sortedNews().find((entry) => selectedCategory === "todas" || entry.category === selectedCategory);
  currentArticle = article ?? null;
  if (!article) {
    reader.append(emptyMessage("Selecciona una noticia para leerla."));
    return;
  }
  const header = element("div", "article-header");
  header.dataset.category = article.category;
  if (article.image) {
    const image = document.createElement("img");
    image.src = article.image;
    image.alt = "";
    image.className = "article-image";
    header.append(image);
  } else {
    header.append(createWatermarkLogo());
  }
  const articleBody = element("div", "article-content");
  const meta = element("div", "article-meta");
  const time = element("time", "news-date", formatDate(article.date));
  time.dateTime = article.date;
  meta.append(createCategoryBadge(article.category), time);
  articleBody.append(meta, heading("h1", article.title));
  for (const block of article.body) articleBody.append(renderBlock(block));
  if (article.link) articleBody.append(button("button button-secondary web-link", "Ver en la web", () => actions.openExternal(article.link!)));
  reader.append(header, articleBody);
}

function renderBlock(block: NewsBlock): HTMLElement {
  if (block.type === "heading") return renderBoldText("h3", block.text);
  if (block.type === "paragraph") return renderBoldText("p", block.text);
  const list = element("ul", "article-list");
  for (const item of block.items) list.append(renderBoldText("li", item));
  return list;
}

function renderBoldText(tag: "h3" | "p" | "li", text: string): HTMLElement {
  const node = document.createElement(tag);
  const pattern = /\*\*(.+?)\*\*/g;
  let cursor = 0;
  for (const match of text.matchAll(pattern)) {
    const start = match.index ?? 0;
    node.append(document.createTextNode(text.slice(cursor, start)));
    const strong = document.createElement("strong");
    strong.textContent = match[1];
    node.append(strong);
    cursor = start + match[0].length;
  }
  node.append(document.createTextNode(text.slice(cursor)));
  return node;
}

function createNewsIndexItem(article: NewsArticle, onSelect: () => void, selected = false): HTMLElement {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "news-index-item";
  button.dataset.selected = String(selected);
  button.addEventListener("click", onSelect);
  const metadata = element("span", "news-index-meta");
  const time = element("time", "news-date", formatDate(article.date));
  time.dateTime = article.date;
  metadata.append(createCategoryBadge(article.category), time);
  button.append(metadata, heading("h3", article.title));
  return button;
}

function createCategoryBadge(category: NewsCategory): HTMLElement {
  const badge = element("span", "category-badge", categoryNames[category]);
  badge.dataset.category = category;
  badge.append(createThemedIcon(category === "evento" ? "swords" : category === "parche" ? "runes" : category === "mantenimiento" ? "shield" : "scroll"));
  return badge;
}

function createThemedIcon(icon: HighlightIcon | "external" | "download" | "folder"): SVGSVGElement {
  const paths: Record<typeof icon, string> = {
    runes: "M12 2 5 6v12l7 4 7-4V6l-7-4Zm0 4v12m-5-9 10 6M17 9 7 15",
    swords: "m5 3 16 16M19 3 3 19m1-14 4 1-3 3-1-4Zm12 12 4 1-3 3-1-4Z",
    shield: "M12 2 20 5v6c0 5-3.5 8-8 11-4.5-3-8-6-8-11V5l8-3Zm-4 10 3 3 5-6",
    scroll: "M6 3h12v18H6a3 3 0 0 1-3-3V6a3 3 0 0 1 3-3Zm0 0v15a3 3 0 0 0 3 3m1-13h5m-5 4h5",
    external: "M14 4h6v6m-1-5-9 9M18 13v6H4V5h6",
    download: "M12 3v12m-5-5 5 5 5-5M4 19h16",
    folder: "M3 6h7l2 2h9v11H3zM3 9h18",
  };
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("fill", "none");
  svg.setAttribute("stroke", "currentColor");
  svg.setAttribute("stroke-width", "1.5");
  svg.setAttribute("stroke-linecap", "round");
  svg.setAttribute("stroke-linejoin", "round");
  svg.setAttribute("aria-hidden", "true");
  const path = document.createElementNS("http://www.w3.org/2000/svg", "path");
  path.setAttribute("d", paths[icon]);
  svg.append(path);
  return svg;
}

function createWatermarkLogo(): HTMLElement {
  const watermark = element("div", "logo-watermark");
  const image = document.createElement("img");
  image.src = new URL("./assets/logo-warcrafted.jpg", import.meta.url).href;
  image.alt = "";
  watermark.append(image);
  return watermark;
}

function setupTabs(): void {
  const tablist = requiredElement<HTMLElement>("#tab-list");
  const tabs = [...tablist.querySelectorAll<HTMLButtonElement>("[role=tab]")];
  for (const tab of tabs) tab.addEventListener("click", () => activateTab(tab.id.replace("tab-", "")));
  tablist.addEventListener("keydown", (event: KeyboardEvent) => {
    if (event.key !== "ArrowRight" && event.key !== "ArrowLeft") return;
    event.preventDefault();
    const current = tabs.findIndex((tab) => tab.getAttribute("aria-selected") === "true");
    const delta = event.key === "ArrowRight" ? 1 : -1;
    const next = tabs[(current + delta + tabs.length) % tabs.length];
    activateTab(next.id.replace("tab-", ""));
    next.focus();
  });
  let saved = "home";
  try {
    const stored = localStorage.getItem("warcrafted.activeTab");
    if (stored && ["home", "news", "addons", "settings"].includes(stored)) saved = stored;
  } catch {
    // El almacenamiento local puede estar desactivado; se mantiene la pestaña Inicio.
  }
  activateTab(saved);
}

function activateTab(name: string): void {
  const validTabs = ["home", "news", "addons", "settings"];
  if (!validTabs.includes(name)) return;
  for (const tabName of validTabs) {
    const active = tabName === name;
    const tab = requiredElement<HTMLButtonElement>(`#tab-${tabName}`);
    const panel = requiredElement<HTMLElement>(`#panel-${tabName}`);
    tab.setAttribute("aria-selected", String(active));
    tab.tabIndex = active ? 0 : -1;
    panel.hidden = !active;
  }
  try {
    localStorage.setItem("warcrafted.activeTab", name);
  } catch {
    // La navegación sigue funcionando aunque localStorage no esté disponible.
  }
}

function openArticle(id: string): void {
  selectedNewsId = id;
  selectedCategory = "todas";
  activateTab("news");
  const index = requiredElement<HTMLElement>(".news-index");
  renderNewsIndex(index);
  renderArticle(requiredElement<HTMLElement>("#news-reader"), { openExternal: openArticleUrl });
}

function openArticleUrl(url: string): void {
  window.dispatchEvent(new CustomEvent("warcrafted-open-url", { detail: url }));
}

function sortedNews(): NewsArticle[] {
  return [...content.news].sort((left, right) => right.date.localeCompare(left.date));
}

function formatDate(date: string): string {
  return new Intl.DateTimeFormat("es-ES", { day: "numeric", month: "long", year: "numeric", timeZone: "UTC" })
    .format(new Date(`${date}T12:00:00Z`));
}

// Componente reutilizable para buscar clientes instalados; se usa en el asistente y en Ajustes.
function createClientSearch(actions: ContentActions): HTMLElement {
  const container = element("section", "client-search panel");
  container.append(eyebrow("BÚSQUEDA AUTOMÁTICA"), heading("h2", "Instalaciones encontradas"));
  const status = paragraph(
    "Pulsa «Buscar instalaciones» para rastrear el disco en busca del cliente.",
    "client-search-status",
  );
  const results = element("ul", "client-candidate-list");
  results.hidden = true;
  const search = button("button button-secondary", "Buscar instalaciones", () => void runSearch());
  const cancel = button("button button-secondary", "Cancelar", actions.cancelDetection);
  cancel.hidden = true;
  const controls = element("div", "client-search-actions");
  controls.append(search, cancel);
  container.append(controls, status, results);

  async function runSearch(): Promise<void> {
    search.disabled = true;
    search.textContent = "Buscando…";
    cancel.hidden = false;
    results.hidden = true;
    results.replaceChildren();
    status.textContent = "Buscando instalaciones de World of Warcraft en las carpetas habituales…";
    try {
      renderClients(await actions.detectClients());
    } finally {
      search.disabled = false;
      search.textContent = "Buscar instalaciones";
      cancel.hidden = true;
    }
  }

  function renderClients(clients: DetectedClient[]): void {
    results.replaceChildren();
    if (clients.length === 0) {
      status.textContent =
        "No se encontró ninguna instalación de World of Warcraft. Puedes elegir la carpeta a mano.";
      return;
    }
    status.textContent =
      clients.length === 1
        ? "Se encontró 1 instalación."
        : `Se encontraron ${clients.length} instalaciones.`;
    for (const client of clients) results.append(createCandidateRow(client, actions));
    results.hidden = false;
  }

  return container;
}

function createCandidateRow(client: DetectedClient, actions: ContentActions): HTMLLIElement {
  const row = element("li", "client-candidate");
  row.dataset.valid = String(client.valid);
  const details = element("div", "candidate-details");
  const path = element("span", "candidate-path", client.path);
  path.title = client.path;
  const version = element(
    "span",
    "candidate-version",
    client.version === null ? "Versión del cliente desconocida" : `Cliente ${client.version}`,
  );
  details.append(path, version);
  const badge = element("span", "candidate-badge", client.valid ? "Compatible" : "No compatible");
  const use = button("button button-secondary", "Usar esta carpeta", () =>
    void actions.useClientFolder(client.path),
  );
  row.append(details, badge, use);
  return row;
}

function createPanel(name: string, active = false): HTMLElement {
  const panel = element("section", "tab-panel");
  panel.id = `panel-${name}`;
  panel.setAttribute("role", "tabpanel");
  panel.setAttribute("aria-labelledby", `tab-${name}`);
  panel.tabIndex = 0;
  panel.hidden = !active;
  return panel;
}

function element<K extends keyof HTMLElementTagNameMap>(tag: K, className = "", text = ""): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text) node.textContent = text;
  return node;
}

function heading(tag: "h1" | "h2" | "h3", text: string, id?: string): HTMLElement {
  const node = element(tag, "", text);
  if (id) node.id = id;
  return node;
}

function paragraph(text: string, className = ""): HTMLParagraphElement {
  return element("p", className, text);
}

function eyebrow(text: string): HTMLParagraphElement {
  return paragraph(text, "eyebrow");
}

function button(className: string, text: string, onClick: () => void, id?: string): HTMLButtonElement {
  const node = document.createElement("button");
  node.type = "button";
  node.className = className;
  node.textContent = text;
  if (id) node.id = id;
  node.addEventListener("click", onClick);
  return node;
}

function emptyListItem(text: string): HTMLLIElement {
  const item = document.createElement("li");
  item.className = "empty-state";
  item.textContent = text;
  return item;
}

function emptyMessage(text: string): HTMLParagraphElement {
  const item = document.createElement("p");
  item.className = "empty-state";
  item.textContent = text;
  return item;
}

function requiredElement<T extends HTMLElement>(selector: string): T {
  const element = document.querySelector<T>(selector);
  if (!element) throw new Error(`No se encontró el elemento ${selector}.`);
  return element;
}

let selectedCategory: NewsCategory | "todas" = "todas";
let selectedNewsId: string | null = sortedNews()[0]?.id ?? null;
let currentArticle: NewsArticle | null = null;
let featuredIndex = 0;
