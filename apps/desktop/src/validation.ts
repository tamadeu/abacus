import type { Engine, Validation, ValidationKind, ValidationOp } from "./engine";

const OPERATORS: Record<ValidationOp, string> = {
  between: "entre",
  notBetween: "fora de",
  equal: "igual a",
  notEqual: "diferente de",
  greater: "maior que",
  greaterEqual: "maior ou igual a",
  less: "menor que",
  lessEqual: "menor ou igual a",
};

const KINDS: Record<ValidationKind, string> = {
  list: "Lista",
  whole: "Número inteiro",
  decimal: "Número decimal",
  textLength: "Tamanho do texto",
};

/** Texto da regra na lista do painel. */
export function describeValidation(rule: Validation): string {
  if (rule.kind === "list") return `Lista: ${rule.value}`;
  const between = rule.operator === "between" || rule.operator === "notBetween";
  const limit = between ? `${rule.value} e ${rule.value2}` : rule.value;
  return `${KINDS[rule.kind]} ${OPERATORS[rule.operator]} ${limit}`;
}

interface Deps {
  engine: Engine;
  sheet: () => number;
  /** A seleção como texto (`A1:B5`). */
  selection: () => string;
  /** Avisa que as regras mudaram, para marcar o arquivo e redesenhar. */
  changed: () => Promise<void>;
}

/** Liga o painel de validação de dados; devolve a função que o abre. */
export function setupValidationPanel({ engine, sheet, selection, changed }: Deps): () => Promise<void> {
  const $ = <T extends HTMLElement>(selector: string) => document.querySelector<T>(selector) as T;
  const panel = $("#dv-panel");
  const list = $("#dv-list");
  const formTitle = $("#dv-form-title");
  const range = $<HTMLInputElement>("#dv-range");
  const kind = $<HTMLSelectElement>("#dv-kind");
  const op = $<HTMLSelectElement>("#dv-op");
  const opLabel = $("#dv-op-label");
  const value = $<HTMLInputElement>("#dv-value");
  const value2 = $<HTMLInputElement>("#dv-value2");
  const message = $<HTMLInputElement>("#dv-message");
  const error = $("#dv-error");
  const save = $("#dv-save");
  const cancel = $("#dv-cancel");

  let rules: Validation[] = [];
  /** Regra em edição; `null` ao criar uma nova. */
  let editing: Validation | null = null;

  const fail = (reason: unknown) => {
    error.textContent = String(reason);
    error.hidden = false;
  };

  function renderList() {
    list.replaceChildren(
      ...rules.map((rule) => {
        const item = document.createElement("li");
        const text = document.createElement("span");
        text.className = "cond-text";
        text.textContent = `${rule.range} · ${describeValidation(rule)}`;
        text.title = text.textContent;
        const edit = document.createElement("button");
        edit.type = "button";
        edit.textContent = "Editar";
        edit.addEventListener("click", () => {
          fillForm(rule);
          kind.focus();
        });
        const remove = document.createElement("button");
        remove.type = "button";
        remove.textContent = "Excluir";
        remove.addEventListener("click", () => void removeRule(rule));
        item.append(text, edit, remove);
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

  /** Mostra só os campos que o tipo e a condição escolhidos usam. */
  function syncForm() {
    const isList = kind.value === "list";
    const between = op.value === "between" || op.value === "notBetween";
    opLabel.hidden = isList;
    value.hidden = false;
    value2.hidden = isList || !between;
    value.placeholder = isList ? "Itens separados por ; (Sim; Não) ou =A1:A5" : between ? "Mínimo" : "Valor";
    value2.placeholder = "Máximo";
  }

  function fillForm(rule: Validation | null) {
    editing = rule;
    error.hidden = true;
    formTitle.textContent = rule ? "Editar regra" : "Nova regra";
    save.textContent = rule ? "Salvar alterações" : "Adicionar regra";
    cancel.hidden = rule === null;
    range.value = rule?.range ?? selection();
    kind.value = rule?.kind ?? "list";
    op.value = rule?.operator ?? "between";
    value.value = rule?.value ?? "";
    value2.value = rule?.value2 ?? "";
    message.value = rule?.message ?? "";
    syncForm();
  }

  async function reload() {
    try {
      rules = await engine.validations(sheet());
    } catch (reason) {
      rules = [];
      fail(reason);
    }
    renderList();
  }

  async function removeRule(rule: Validation) {
    try {
      await engine.deleteValidation(rule.id);
    } catch (reason) {
      fail(reason);
      return;
    }
    await changed();
    await reload();
    fillForm(null);
  }

  async function submit() {
    error.hidden = true;
    const rule: Validation = {
      id: editing?.id ?? 0,
      sheet: sheet(),
      range: range.value.trim(),
      kind: kind.value as ValidationKind,
      operator: kind.value === "list" ? "between" : (op.value as ValidationOp),
      value: value.value.trim(),
      value2: value2.hidden ? "" : value2.value.trim(),
      message: message.value.trim(),
    };
    try {
      if (editing) await engine.updateValidation(sheet(), rule);
      else await engine.addValidation(sheet(), rule);
    } catch (reason) {
      fail(reason);
      return;
    }
    await changed();
    await reload();
    fillForm(null);
  }

  kind.addEventListener("change", syncForm);
  op.addEventListener("change", syncForm);
  save.addEventListener("click", () => void submit());
  cancel.addEventListener("click", () => fillForm(null));
  $("#dv-close").addEventListener("click", () => {
    panel.hidden = true;
    document.querySelector<HTMLElement>("#grid")?.focus();
  });
  panel.addEventListener("keydown", (event) => {
    if (event.key === "Escape") $("#dv-close").click();
    else if (event.key === "Enter" && (event.target as HTMLElement).tagName === "INPUT") {
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
