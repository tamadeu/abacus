import type { CondKind, CondRule, Engine } from "./engine";

/** O que cada tipo de regra pede no formulário. */
interface KindForm {
  label: string;
  /** Quantos valores a regra pede. */
  values: 0 | 1 | 2;
  placeholder?: string;
  /** Como a regra pinta: o formato da célula, uma escala de cor ou uma barra de dados. */
  look: "format" | "scale" | "bar";
  /** Texto da regra na lista, dado o valor e o segundo valor. */
  describe: (value: string, value2: string) => string;
}

const KINDS: Record<Exclude<CondKind, "other">, KindForm> = {
  greater: { label: "Maior que", values: 1, placeholder: "Valor ou célula (100, B1)", look: "format", describe: (v) => `Maior que ${v}` },
  greaterEqual: { label: "Maior ou igual a", values: 1, placeholder: "Valor ou célula (100, B1)", look: "format", describe: (v) => `Maior ou igual a ${v}` },
  less: { label: "Menor que", values: 1, placeholder: "Valor ou célula (100, B1)", look: "format", describe: (v) => `Menor que ${v}` },
  lessEqual: { label: "Menor ou igual a", values: 1, placeholder: "Valor ou célula (100, B1)", look: "format", describe: (v) => `Menor ou igual a ${v}` },
  equal: { label: "Igual a", values: 1, placeholder: "Valor ou célula (100, B1)", look: "format", describe: (v) => `Igual a ${v}` },
  notEqual: { label: "Diferente de", values: 1, placeholder: "Valor ou célula (100, B1)", look: "format", describe: (v) => `Diferente de ${v}` },
  between: { label: "Entre", values: 2, placeholder: "Valor", look: "format", describe: (v, w) => `Entre ${v} e ${w}` },
  notBetween: { label: "Fora do intervalo", values: 2, placeholder: "Valor", look: "format", describe: (v, w) => `Fora de ${v} a ${w}` },
  contains: { label: "Texto que contém", values: 1, placeholder: "Texto", look: "format", describe: (v) => `Contém “${v}”` },
  notContains: { label: "Texto que não contém", values: 1, placeholder: "Texto", look: "format", describe: (v) => `Não contém “${v}”` },
  beginsWith: { label: "Texto que começa com", values: 1, placeholder: "Texto", look: "format", describe: (v) => `Começa com “${v}”` },
  endsWith: { label: "Texto que termina com", values: 1, placeholder: "Texto", look: "format", describe: (v) => `Termina com “${v}”` },
  duplicates: { label: "Valores duplicados", values: 0, look: "format", describe: () => "Valores duplicados" },
  unique: { label: "Valores únicos", values: 0, look: "format", describe: () => "Valores únicos" },
  blanks: { label: "Células vazias", values: 0, look: "format", describe: () => "Células vazias" },
  notBlanks: { label: "Células preenchidas", values: 0, look: "format", describe: () => "Células preenchidas" },
  errors: { label: "Células com erro", values: 0, look: "format", describe: () => "Células com erro" },
  aboveAverage: { label: "Acima da média", values: 0, look: "format", describe: () => "Acima da média" },
  belowAverage: { label: "Abaixo da média", values: 0, look: "format", describe: () => "Abaixo da média" },
  top: { label: "Os maiores valores", values: 1, placeholder: "Quantos (10)", look: "format", describe: (v) => `Os ${v} maiores` },
  bottom: { label: "Os menores valores", values: 1, placeholder: "Quantos (10)", look: "format", describe: (v) => `Os ${v} menores` },
  colorScale: { label: "Escala de cor", values: 0, look: "scale", describe: () => "Escala de cor" },
  dataBar: { label: "Barra de dados", values: 0, look: "bar", describe: () => "Barra de dados" },
};

/** Grupos do seletor de tipo, na ordem em que aparecem. */
const GROUPS: [string, Exclude<CondKind, "other">[]][] = [
  ["Valores", ["greater", "greaterEqual", "less", "lessEqual", "equal", "notEqual", "between", "notBetween"]],
  ["Texto", ["contains", "notContains", "beginsWith", "endsWith"]],
  ["Outras", ["duplicates", "unique", "blanks", "notBlanks", "errors", "aboveAverage", "belowAverage", "top", "bottom"]],
  ["Visuais", ["colorScale", "dataBar"]],
];

/** Texto da regra na lista. */
export function describeRule(rule: CondRule): string {
  if (rule.kind === "other") return "Regra avançada (só dá para apagar)";
  return KINDS[rule.kind].describe(rule.value, rule.value2);
}

const DEFAULT_FILL = "#ffc7ce";
const DEFAULT_TEXT = "#9c0006";
const DEFAULT_LOW = "#ffffff";
const DEFAULT_HIGH = "#63be7b";
const DEFAULT_MID = "#ffeb84";
const DEFAULT_BAR = "#638ec6";

interface Deps {
  engine: Engine;
  sheet: () => number;
  /** A seleção como texto (`A1:B5`). */
  selection: () => string;
  /** Avisa que as regras mudaram, para marcar o arquivo e redesenhar. */
  changed: () => Promise<void>;
}

/** Liga o painel de formatação condicional; devolve a função que o abre. */
export function setupConditionalPanel({ engine, sheet, selection, changed }: Deps): () => Promise<void> {
  const $ = <T extends HTMLElement>(selector: string) => document.querySelector<T>(selector) as T;
  const panel = $("#cond-panel");
  const list = $("#cond-list");
  const formTitle = $("#cond-form-title");
  const range = $<HTMLInputElement>("#cond-range");
  const kind = $<HTMLSelectElement>("#cond-kind");
  const value = $<HTMLInputElement>("#cond-value");
  const value2 = $<HTMLInputElement>("#cond-value2");
  const fill = $<HTMLInputElement>("#cond-fill");
  const color = $<HTMLInputElement>("#cond-color");
  const mid = $<HTMLInputElement>("#cond-mid");
  const useMid = $<HTMLInputElement>("#cond-use-mid");
  const bold = $<HTMLInputElement>("#cond-bold");
  const error = $("#cond-error");
  const save = $("#cond-save");
  const cancel = $("#cond-cancel");
  const fillLabel = $("#cond-fill-label");
  const colorLabel = $("#cond-color-label");

  let rules: CondRule[] = [];
  /** Regra em edição; `null` ao criar uma nova. */
  let editing: CondRule | null = null;
  /** O tipo de aparência do formulário agora, para trocar as cores padrão quando ele muda. */
  let lastLook: KindForm["look"] = "format";

  for (const [group, kinds] of GROUPS) {
    const optgroup = document.createElement("optgroup");
    optgroup.label = group;
    for (const one of kinds) optgroup.append(new Option(KINDS[one].label, one));
    kind.append(optgroup);
  }

  const swatches = (rule: CondRule) => {
    const box = document.createElement("span");
    box.className = "cond-swatch";
    if (rule.kind === "colorScale") {
      const stops = [rule.color, rule.mid, rule.fill].filter(Boolean);
      box.style.background = `linear-gradient(90deg, ${stops.join(", ")})`;
    } else {
      box.style.background = rule.fill ?? "#fff";
      box.style.color = rule.color ?? "inherit";
      box.style.fontWeight = rule.bold ? "700" : "400";
      box.textContent = rule.kind === "dataBar" ? "" : "Aa";
    }
    return box;
  };

  function renderList() {
    list.replaceChildren(
      ...rules.map((rule) => {
        const item = document.createElement("li");
        const text = document.createElement("span");
        text.className = "cond-text";
        text.textContent = `${rule.range} · ${describeRule(rule)}`;
        const edit = document.createElement("button");
        edit.type = "button";
        edit.textContent = "Editar";
        edit.disabled = rule.kind === "other";
        edit.addEventListener("click", () => startEdit(rule));
        const remove = document.createElement("button");
        remove.type = "button";
        remove.textContent = "Excluir";
        remove.addEventListener("click", () => void removeRule(rule));
        item.append(swatches(rule), text, edit, remove);
        return item;
      }),
    );
    if (rules.length === 0) {
      const empty = document.createElement("li");
      empty.className = "cond-empty";
      empty.textContent = "Nenhuma regra nesta planilha.";
      list.append(empty);
    }
  }

  /** Mostra só os campos que o tipo escolhido usa. */
  function syncForm() {
    const form = KINDS[kind.value as Exclude<CondKind, "other">];
    value.hidden = form.values < 1;
    value2.hidden = form.values < 2;
    value.placeholder = form.placeholder ?? "";
    value2.placeholder = "e";
    for (const element of document.querySelectorAll<HTMLElement>("[data-cond-look]")) {
      const looks = element.dataset.condLook!.split(" ");
      element.hidden = !looks.includes(form.look);
    }
    fillLabel.textContent = form.look === "scale" ? "Maior valor" : form.look === "bar" ? "Cor da barra" : "Fundo";
    colorLabel.textContent = form.look === "scale" ? "Menor valor" : "Texto";
    mid.disabled = !useMid.checked;
  }

  /** Ao trocar para outra aparência (formato, escala, barra), volta às cores padrão dela. */
  function changeKind() {
    const look = KINDS[kind.value as Exclude<CondKind, "other">].look;
    if (look !== lastLook) {
      lastLook = look;
      fill.value = look === "scale" ? DEFAULT_HIGH : look === "bar" ? DEFAULT_BAR : DEFAULT_FILL;
      color.value = look === "scale" ? DEFAULT_LOW : DEFAULT_TEXT;
    }
    syncForm();
  }

  function fillForm(rule: CondRule | null) {
    editing = rule;
    error.hidden = true;
    formTitle.textContent = rule ? "Editar regra" : "Nova regra";
    save.textContent = rule ? "Salvar alterações" : "Adicionar regra";
    cancel.hidden = rule === null;
    range.value = rule?.range ?? selection();
    kind.value = rule && rule.kind !== "other" ? rule.kind : "greater";
    value.value = rule?.value ?? "";
    value2.value = rule?.value2 ?? "";
    const scale = rule?.kind === "colorScale";
    fill.value = rule?.fill ?? (scale ? DEFAULT_HIGH : rule?.kind === "dataBar" ? DEFAULT_BAR : DEFAULT_FILL);
    color.value = rule?.color ?? (scale ? DEFAULT_LOW : DEFAULT_TEXT);
    useMid.checked = Boolean(rule?.mid);
    mid.value = rule?.mid ?? DEFAULT_MID;
    bold.checked = rule?.bold ?? false;
    lastLook = KINDS[kind.value as Exclude<CondKind, "other">].look;
    syncForm();
  }

  function startEdit(rule: CondRule) {
    fillForm(rule);
    kind.focus();
  }

  async function reload() {
    try {
      rules = await engine.condRules(sheet());
    } catch (reason) {
      rules = [];
      error.textContent = String(reason);
      error.hidden = false;
    }
    renderList();
  }

  async function removeRule(rule: CondRule) {
    if (rule.index === null) return;
    try {
      await engine.deleteCondRule(sheet(), rule.index);
    } catch (reason) {
      error.textContent = String(reason);
      error.hidden = false;
      return;
    }
    await changed();
    await reload();
    fillForm(null);
  }

  /** A regra como o formulário a descreve. */
  function read(): CondRule {
    const form = KINDS[kind.value as Exclude<CondKind, "other">];
    const format = form.look === "format";
    return {
      index: editing?.index ?? null,
      range: range.value.trim(),
      kind: kind.value as CondKind,
      value: form.values >= 1 ? value.value.trim() : "",
      value2: form.values >= 2 ? value2.value.trim() : "",
      fill: fill.value,
      color: format || form.look === "scale" ? color.value : null,
      mid: form.look === "scale" && useMid.checked ? mid.value : null,
      bold: format && bold.checked,
    };
  }

  async function submit() {
    error.hidden = true;
    const rule = read();
    try {
      if (editing) await engine.updateCondRule(sheet(), rule);
      else await engine.addCondRule(sheet(), rule);
    } catch (reason) {
      error.textContent = String(reason);
      error.hidden = false;
      return;
    }
    await changed();
    await reload();
    fillForm(null);
  }

  kind.addEventListener("change", changeKind);
  useMid.addEventListener("change", syncForm);
  save.addEventListener("click", () => void submit());
  cancel.addEventListener("click", () => fillForm(null));
  $("#cond-close").addEventListener("click", () => {
    panel.hidden = true;
    document.querySelector<HTMLElement>("#grid")?.focus();
  });
  panel.addEventListener("keydown", (event) => {
    if (event.key === "Escape") $("#cond-close").click();
    else if (event.key === "Enter" && (event.target as HTMLElement).tagName === "INPUT" && (event.target as HTMLInputElement).type === "text") {
      event.preventDefault();
      void submit();
    }
  });

  return async () => {
    panel.hidden = false;
    await reload();
    fillForm(null);
    range.focus();
    range.select();
  };
}
