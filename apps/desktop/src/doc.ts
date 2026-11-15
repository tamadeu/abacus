import { invoke, isTauri } from "@tauri-apps/api/core";
import { ask, open, save } from "@tauri-apps/plugin-dialog";

/** O documento como o motor o entende (`abacus-doc`). */
interface Run {
  text: string;
  bold: boolean;
  italic: boolean;
  underline: boolean;
  strike: boolean;
}
type Align = "Left" | "Center" | "Right" | "Justify";
type Kind = "Normal" | "Bullet" | "Numbered" | { Heading: number };
interface Paragraph {
  kind: Kind;
  align: Align;
  runs: Run[];
}
interface DocModel {
  blocks: { Paragraph: Paragraph }[];
}

const FILTER = { name: "Documento do Word", extensions: ["docx"] };
const ALIGN_STYLE: Record<Align, string> = { Left: "", Center: "center", Right: "right", Justify: "justify" };

const escapeHtml = (text: string) => text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

function runHtml(run: Run): string {
  let html = escapeHtml(run.text).replace(/\n/g, "<br>");
  if (run.bold) html = `<b>${html}</b>`;
  if (run.italic) html = `<i>${html}</i>`;
  if (run.underline) html = `<u>${html}</u>`;
  if (run.strike) html = `<s>${html}</s>`;
  return html;
}

/** O modelo em HTML editável. Listas seguidas do mesmo tipo viram uma lista só. */
export function toHtml(doc: DocModel): string {
  let out = "";
  let open: "ul" | "ol" | null = null;
  for (const { Paragraph: p } of doc.blocks) {
    const list = p.kind === "Bullet" ? "ul" : p.kind === "Numbered" ? "ol" : null;
    if (open && open !== list) {
      out += `</${open}>`;
      open = null;
    }
    if (list && !open) {
      out += `<${list}>`;
      open = list;
    }
    const style = ALIGN_STYLE[p.align] ? ` style="text-align:${ALIGN_STYLE[p.align]}"` : "";
    const inner = p.runs.map(runHtml).join("") || "<br>";
    if (list) out += `<li${style}>${inner}</li>`;
    else if (typeof p.kind === "object") out += `<h${p.kind.Heading}${style}>${inner}</h${p.kind.Heading}>`;
    else out += `<p${style}>${inner}</p>`;
  }
  if (open) out += `</${open}>`;
  return out;
}

const BLOCKS = new Set(["P", "DIV", "H1", "H2", "H3", "H4", "H5", "H6", "LI"]);

/** Lê o HTML do editor de volta como modelo. */
export function fromHtml(root: HTMLElement): DocModel {
  const blocks: DocModel["blocks"] = [];
  const add = (element: HTMLElement, kind: Kind) => {
    const align = ((): Align => {
      switch (element.style.textAlign || element.getAttribute("align")) {
        case "center":
          return "Center";
        case "right":
          return "Right";
        case "justify":
          return "Justify";
        default:
          return "Left";
      }
    })();
    const runs: Run[] = [];
    walk(element, { bold: false, italic: false, underline: false, strike: false }, runs);
    blocks.push({ Paragraph: { kind, align, runs } });
  };
  const visit = (parent: HTMLElement) => {
    // Texto solto, sem parágrafo em volta, vira um parágrafo.
    let loose: HTMLElement | null = null;
    const flush = () => {
      if (loose) add(loose, "Normal");
      loose = null;
    };
    for (const node of Array.from(parent.childNodes)) {
      if (node.nodeType === Node.ELEMENT_NODE && /^(UL|OL)$/.test((node as HTMLElement).tagName)) {
        flush();
        const kind: Kind = (node as HTMLElement).tagName === "UL" ? "Bullet" : "Numbered";
        for (const item of Array.from(node.childNodes)) {
          if ((item as HTMLElement).tagName === "LI") add(item as HTMLElement, kind);
        }
      } else if (node.nodeType === Node.ELEMENT_NODE && BLOCKS.has((node as HTMLElement).tagName)) {
        flush();
        const element = node as HTMLElement;
        const heading = /^H([1-6])$/.exec(element.tagName);
        // O navegador às vezes põe listas e parágrafos dentro de um <p>: aí ele é só um recipiente.
        if (element.querySelector("p, div, ul, ol, h1, h2, h3, h4, h5, h6")) visit(element);
        else add(element, heading ? { Heading: Number(heading[1]) } : "Normal");
      } else {
        loose ??= document.createElement("p");
        loose.appendChild(node.cloneNode(true));
      }
    }
    flush();
  };
  visit(root);
  if (blocks.length === 0) blocks.push({ Paragraph: { kind: "Normal", align: "Left", runs: [] } });
  return { blocks };
}

type Format = Omit<Run, "text">;

function walk(node: Node, format: Format, runs: Run[]) {
  for (const child of Array.from(node.childNodes)) {
    if (child.nodeType === Node.TEXT_NODE) {
      const text = (child.textContent ?? "").replace(/ /g, " ");
      if (text) push(runs, text, format);
    } else if (child.nodeType === Node.ELEMENT_NODE) {
      const element = child as HTMLElement;
      const tag = element.tagName;
      if (tag === "BR") {
        // O <br> que só segura um parágrafo vazio não é quebra de linha.
        if (element.nextSibling || element.previousSibling) push(runs, "\n", format);
        continue;
      }
      const next = { ...format };
      if (tag === "B" || tag === "STRONG" || /^(bold|[6-9]00)$/.test(element.style.fontWeight)) next.bold = true;
      if (tag === "I" || tag === "EM" || element.style.fontStyle === "italic") next.italic = true;
      if (tag === "U" || element.style.textDecorationLine.includes("underline")) next.underline = true;
      if (tag === "S" || tag === "STRIKE" || tag === "DEL" || element.style.textDecorationLine.includes("line-through")) {
        next.strike = true;
      }
      walk(element, next, runs);
    }
  }
}

function push(runs: Run[], text: string, format: Format) {
  const last = runs[runs.length - 1];
  if (last && last.bold === format.bold && last.italic === format.italic && last.underline === format.underline && last.strike === format.strike) {
    last.text += text;
  } else {
    runs.push({ text, ...format });
  }
}

/** Liga o editor de documentos. Devolve o que a janela principal usa para abri-lo. */
export function setupDocument(): { show(path?: string): Promise<void> } {
  const app = document.querySelector<HTMLElement>("#doc-app")!;
  const page = document.querySelector<HTMLElement>("#doc-page")!;
  const nameEl = document.querySelector<HTMLElement>("#doc-title")!;
  const stateEl = document.querySelector<HTMLElement>("#doc-info")!;
  const headingSelect = document.querySelector<HTMLSelectElement>("#doc-heading")!;
  let path: string | null = null;
  let dirty = false;

  const fileName = () => (path ? (path.split(/[\\/]/).pop() ?? path) : "Documento sem título");
  const refreshTitle = () => {
    nameEl.textContent = `${fileName()}${dirty ? " •" : ""}`;
    const text = page.innerText.trim();
    const words = text ? text.split(/\s+/).length : 0;
    stateEl.textContent = `${words} ${words === 1 ? "palavra" : "palavras"}`;
  };
  const load = (doc: DocModel | null) => {
    page.innerHTML = doc ? toHtml(doc) : "<p><br></p>";
    dirty = false;
    refreshTitle();
    page.focus();
  };
  const confirmDiscard = async () =>
    !dirty || (await ask("Há alterações não salvas. Descartá-las?", { title: "Abacus", kind: "warning", okLabel: "Descartar", cancelLabel: "Cancelar" }));

  async function openPath(target: string) {
    try {
      load(await invoke<DocModel>("doc_open", { path: target }));
      path = target;
      refreshTitle();
    } catch (error) {
      stateEl.textContent = `Erro ao abrir: ${error}`;
    }
  }

  async function saveDoc(askWhere: boolean) {
    if (!isTauri()) {
      stateEl.textContent = "Salvar só funciona no aplicativo";
      return;
    }
    let target = askWhere || !path ? await save({ filters: [FILTER], defaultPath: `${fileName().replace(/\.docx$/i, "")}.docx` }) : path;
    if (target === null) return;
    if (!target.toLowerCase().endsWith(".docx")) target += ".docx";
    try {
      await invoke("doc_save", { path: target, doc: fromHtml(page) });
      path = target;
      dirty = false;
      refreshTitle();
      stateEl.textContent = "Salvo";
    } catch (error) {
      stateEl.textContent = `Erro ao salvar: ${error}`;
    }
  }

  const command = (name: string, value?: string) => {
    page.focus();
    document.execCommand(name, false, value);
    dirty = true;
    refreshTitle();
  };

  document.execCommand("defaultParagraphSeparator", false, "p");
  page.addEventListener("input", () => {
    dirty = true;
    refreshTitle();
  });
  // Cola só o texto: o que vem de fora traz estilos que o documento não guarda.
  page.addEventListener("paste", (event) => {
    event.preventDefault();
    const text = event.clipboardData?.getData("text/plain") ?? "";
    document.execCommand("insertText", false, text);
  });
  page.addEventListener("keydown", (event) => {
    if (!(event.ctrlKey || event.metaKey)) return;
    const key = event.key.toLowerCase();
    if (key === "s") {
      event.preventDefault();
      void saveDoc(event.shiftKey);
    } else if (key === "b" || key === "i" || key === "u") {
      // O editor já faz isso, e `execCommand` mantém o desfazer.
    }
  });
  page.addEventListener("keyup", syncControls);
  page.addEventListener("mouseup", syncControls);

  function syncControls() {
    const block = document.queryCommandValue("formatBlock").toLowerCase();
    headingSelect.value = /^h[1-3]$/.test(block) ? block : "p";
    for (const button of app.querySelectorAll<HTMLButtonElement>("[data-cmd]")) {
      const cmd = button.dataset.cmd!;
      if (["bold", "italic", "underline", "strikeThrough", "insertUnorderedList", "insertOrderedList"].includes(cmd)) {
        button.classList.toggle("on", document.queryCommandState(cmd));
      }
    }
  }

  for (const button of app.querySelectorAll<HTMLButtonElement>("[data-cmd]")) {
    // `mousedown` e não `click`, para o texto não perder a seleção.
    button.addEventListener("mousedown", (event) => {
      event.preventDefault();
      command(button.dataset.cmd!);
      syncControls();
    });
  }
  headingSelect.addEventListener("change", () => {
    command("formatBlock", headingSelect.value);
    syncControls();
  });

  document.querySelector("#doc-new")!.addEventListener("click", async () => {
    if (!(await confirmDiscard())) return;
    path = null;
    load(null);
  });
  document.querySelector("#doc-open")!.addEventListener("click", async () => {
    if (!isTauri()) {
      stateEl.textContent = "Abrir só funciona no aplicativo";
      return;
    }
    if (!(await confirmDiscard())) return;
    const chosen = await open({ multiple: false, directory: false, filters: [FILTER] });
    if (chosen !== null) await openPath(chosen);
  });
  document.querySelector("#doc-save")!.addEventListener("click", () => void saveDoc(false));
  document.querySelector("#doc-save-as")!.addEventListener("click", () => void saveDoc(true));
  document.querySelector("#doc-close")!.addEventListener("click", async () => {
    if (!(await confirmDiscard())) return;
    app.hidden = true;
  });

  return {
    async show(target) {
      app.hidden = false;
      if (target) await openPath(target);
      else if (page.innerHTML.trim() === "") load(null);
      page.focus();
    },
  };
}
