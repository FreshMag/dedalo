// The words of the editor's interface, in English and Italian.
//
// Diagnostics and the reference come from the core and stay in English.
// Markup carries keys: `data-i18n` sets an element's text, and
// `data-i18n-title` and `data-i18n-aria-label` set those attributes.

export const LANGUAGES = { en: "English", it: "Italiano" };

const STRINGS = {
  en: {
    "skip": "Skip to the editor",
    "open": "Open",
    "open.title": "Open a .dedalo file",
    "save": "Save",
    "save.title": "Save the source as a file",
    "example": "Example",
    "example.choose": "Choose…",
    "example.starter": "Starter",
    "example.crypt": "The forgotten crypt",
    "example.shapes": "Four shapes",
    "example.rotated": "Rotated",
    "example.tower": "A tower on three levels",
    "example.load": "Load",
    "example.load.label": "Load the chosen example",
    "format": "Format",
    "share": "Share",
    "share.title": "Copy a link that holds the whole dungeon",
    "audience": "Audience",
    "from-export": "From export",
    "audience.dm": "DM",
    "audience.player": "Player",
    "audience.clean": "Clean",
    "export": "Export",
    "export.default": "default",
    "level": "Level",
    "level.all": "All levels",
    "rotation": "Rotation",
    "grid": "Grid",
    "labels": "Labels",
    "download": "Download",
    "download.as": "Download as",
    "download.txt": "Text map",
    "download.json": "Document JSON",
    "plugins": "Plugins",
    "reference": "Reference",
    "language": "Language",
    "source": "Source",
    "editor.label": "Dungeon description",
    "editor.help": "Tab indents. To leave the editor with the keyboard, press Escape and then Tab. Control+Space lists completions; Control+I explains the word at the cursor.",
    "panels": "Panels",
    "problems": "Problems",
    "outline": "Outline",
    "textmap": "Text map",
    "preview": "Map preview",
    "preview.help": "The map as an image. Arrow keys move it, plus and minus zoom, 0 fits it. The Text map panel gives the same map as text.",
    "zoom-in": "Zoom in",
    "zoom-out": "Zoom out",
    "fit": "Fit",
    "plugins.about": "Plugins run sandboxed in this page and see only what the map shows. Reviewed plugins come from the index of this site; a plugin from your files or from an address runs only after you choose it.",
    "plugins.folder": "Load a package from a folder",
    "plugins.file": "Load a .dedalopkg file",
    "plugins.url": "Load a .dedalopkg from an address",
    "plugins.url.load": "Load",
    "close": "Close",
    "status.loading": "Loading…",
    "status.ok": "No problems",
    "status.count": "{errors} error(s), {warnings} warning(s)",
    "status.exporting": "Exporting…",
    "status.exported": "Exported",
    "status.export-failed": "Export failed: {reason}",
    "status.has-errors": "the dungeon has errors",
    "status.fix-syntax": "Fix the syntax errors before formatting",
    "status.fix-failed": "The fix could not be applied",
    "status.too-long": "Too long for a link; saving a file instead",
    "status.copied": "Link copied",
    "status.in-bar": "Link in the address bar",
    "status.damaged": "The shared link is damaged",
    "status.plugin-loaded": "Plugin {id} {version} loaded",
    "status.plugin-failed": "The plugin could not be loaded: {reason}",
    "status.choose-folder": "Choose the folder that holds dedalo-plugin.toml",
    "status.choose-example": "Choose an example first",
    "status.choose-format": "Choose a format first",
    "problem.line": "line {line}",
    "problem.none": "No problems.",
    "problem.fix": "Apply fix",
    "problem.go": "Go to line {line}",
    "plugin.session": "{id} (this session only)",
    "plugin.remembered": "{id} (from {origin})",
    "confirm.files": "Load the plugin \"{id}\" from your files? It will run sandboxed in this page.",
    "confirm.index": "This dungeon uses the plugin \"{id}\" {version} from the index of this site ({sha256}). Load it?",
    "confirm.url": "Load the plugin \"{id}\" {version} from {origin}?\n\nHash: {sha256}\n\nIt will run sandboxed in this page.",
    "confirm.shared": "This link asks for the plugin \"{id}\" {version} from {origin}, which is not in the index of this site.\n\nHash: {sha256}\n\nLoad it? It will run sandboxed in this page.",
    "confirm.changed": "The plugin \"{id}\" at {origin} has changed since you loaded it.\n\nNew hash: {sha256}\n\nLoad the new version?",
  },
  it: {
    "skip": "Vai all'editor",
    "open": "Apri",
    "open.title": "Apri un file .dedalo",
    "save": "Salva",
    "save.title": "Salva il sorgente in un file",
    "example": "Esempio",
    "example.choose": "Scegli…",
    "example.starter": "Per iniziare",
    "example.crypt": "La cripta dimenticata",
    "example.shapes": "Quattro forme",
    "example.rotated": "Stanze ruotate",
    "example.tower": "Una torre su tre livelli",
    "example.load": "Carica",
    "example.load.label": "Carica l'esempio scelto",
    "format": "Formatta",
    "share": "Condividi",
    "share.title": "Copia un link che contiene l'intero dungeon",
    "audience": "Pubblico",
    "from-export": "Dall'export",
    "audience.dm": "Master",
    "audience.player": "Giocatori",
    "audience.clean": "Pulita",
    "export": "Export",
    "export.default": "predefinito",
    "level": "Livello",
    "level.all": "Tutti i livelli",
    "rotation": "Rotazione",
    "grid": "Griglia",
    "labels": "Etichette",
    "download": "Scarica",
    "download.as": "Scarica come",
    "download.txt": "Mappa testuale",
    "download.json": "Documento JSON",
    "plugins": "Plugin",
    "reference": "Guida",
    "language": "Lingua",
    "source": "Sorgente",
    "editor.label": "Descrizione del dungeon",
    "editor.help": "Tab rientra il testo. Per uscire dall'editor con la tastiera, premi Esc e poi Tab. Ctrl+Spazio elenca i completamenti; Ctrl+I spiega la parola sotto il cursore.",
    "panels": "Pannelli",
    "problems": "Problemi",
    "outline": "Struttura",
    "textmap": "Mappa testuale",
    "preview": "Anteprima della mappa",
    "preview.help": "La mappa come immagine. Le frecce la spostano, più e meno cambiano lo zoom, 0 la adatta. Il pannello Mappa testuale offre la stessa mappa come testo.",
    "zoom-in": "Ingrandisci",
    "zoom-out": "Riduci",
    "fit": "Adatta",
    "plugins.about": "I plugin girano isolati in questa pagina e vedono solo ciò che la mappa mostra. I plugin verificati vengono dall'indice di questo sito; un plugin dai tuoi file o da un indirizzo gira solo dopo che lo hai scelto.",
    "plugins.folder": "Carica un pacchetto da una cartella",
    "plugins.file": "Carica un file .dedalopkg",
    "plugins.url": "Carica un .dedalopkg da un indirizzo",
    "plugins.url.load": "Carica",
    "close": "Chiudi",
    "status.loading": "Caricamento…",
    "status.ok": "Nessun problema",
    "status.count": "{errors} errori, {warnings} avvisi",
    "status.exporting": "Esportazione…",
    "status.exported": "Esportato",
    "status.export-failed": "Esportazione non riuscita: {reason}",
    "status.has-errors": "il dungeon contiene errori",
    "status.fix-syntax": "Correggi gli errori di sintassi prima di formattare",
    "status.fix-failed": "Non è stato possibile applicare la correzione",
    "status.too-long": "Troppo lungo per un link; salvo un file",
    "status.copied": "Link copiato",
    "status.in-bar": "Il link è nella barra degli indirizzi",
    "status.damaged": "Il link condiviso è danneggiato",
    "status.plugin-loaded": "Plugin {id} {version} caricato",
    "status.plugin-failed": "Non è stato possibile caricare il plugin: {reason}",
    "status.choose-folder": "Scegli la cartella che contiene dedalo-plugin.toml",
    "status.choose-example": "Scegli prima un esempio",
    "status.choose-format": "Scegli prima un formato",
    "problem.line": "riga {line}",
    "problem.none": "Nessun problema.",
    "problem.fix": "Applica la correzione",
    "problem.go": "Vai alla riga {line}",
    "plugin.session": "{id} (solo per questa sessione)",
    "plugin.remembered": "{id} (da {origin})",
    "confirm.files": "Caricare il plugin \"{id}\" dai tuoi file? Girerà isolato in questa pagina.",
    "confirm.index": "Questo dungeon usa il plugin \"{id}\" {version} dell'indice di questo sito ({sha256}). Caricarlo?",
    "confirm.url": "Caricare il plugin \"{id}\" {version} da {origin}?\n\nHash: {sha256}\n\nGirerà isolato in questa pagina.",
    "confirm.shared": "Questo link richiede il plugin \"{id}\" {version} da {origin}, che non è nell'indice di questo sito.\n\nHash: {sha256}\n\nCaricarlo? Girerà isolato in questa pagina.",
    "confirm.changed": "Il plugin \"{id}\" su {origin} è cambiato da quando lo hai caricato.\n\nNuovo hash: {sha256}\n\nCaricare la nuova versione?",
  },
};

let language = "en";

function stored() {
  try {
    return localStorage.getItem("dedalo.lang");
  } catch {
    return null;
  }
}

/** The language in use: the one chosen before, else the browser's. */
export function currentLanguage() {
  return language;
}

export function initLanguage() {
  const wanted = stored() || (navigator.language || "en").slice(0, 2).toLowerCase();
  language = wanted in STRINGS ? wanted : "en";
  return language;
}

export function setLanguage(next) {
  language = next in STRINGS ? next : "en";
  try {
    localStorage.setItem("dedalo.lang", language);
  } catch {
    // Remembering the language is a convenience.
  }
  return language;
}

/** The text for a key, with `{name}` places filled from `values`. */
export function t(key, values = {}) {
  const text = STRINGS[language][key] ?? STRINGS.en[key] ?? key;
  return text.replace(/\{(\w+)\}/g, (_, name) => String(values[name] ?? ""));
}

/** Writes the current language into every element that carries a key. */
export function applyTranslations(root = document) {
  root.documentElement?.setAttribute("lang", language);
  for (const el of root.querySelectorAll("[data-i18n]")) el.textContent = t(el.dataset.i18n);
  for (const el of root.querySelectorAll("[data-i18n-title]")) el.title = t(el.dataset.i18nTitle);
  for (const el of root.querySelectorAll("[data-i18n-aria-label]")) el.setAttribute("aria-label", t(el.dataset.i18nAriaLabel));
}

/** Every key of every language, for the self-test. */
export function keysByLanguage() {
  return Object.fromEntries(Object.entries(STRINGS).map(([lang, words]) => [lang, Object.keys(words).sort()]));
}
