import type {
  Engine,
  Pivot,
  PivotAgg,
  PivotField,
  PivotGroup,
  PivotShowAs,
  PivotSort,
  PivotValue,
} from "./engine";

const AGGS: [PivotAgg, string][] = [
  ["sum", "Soma"],
  ["count", "Contagem"],
  ["countNums", "Contagem de números"],
  ["average", "Média"],
  ["min", "Mínimo"],
  ["max", "Máximo"],
  ["product", "Produto"],
  ["stdDev", "Desvio padrão"],
  ["var", "Variância"],
  ["distinct", "Valores distintos"],
];

const SHOW_AS: [PivotShowAs, string][] = [
  ["normal", "Valor normal"],
  ["pctTotal", "% do total geral"],
  ["pctRow", "% do total da linha"],
  ["pctCol", "% do total da coluna"],
];

const GROUPS: [PivotGroup, string][] = [
  ["none", "Sem agrupamento"],
  ["year", "Ano"],
  ["quarter", "Trimestre"],
  ["month", "Mês"],
  ["yearMonth", "Ano e mês"],
  ["bucket", "Faixas de números"],
];

const SORTS: [PivotSort, string][] = [
  ["labelAsc", "Nome, de A a Z"],
  ["labelDesc", "Nome, de Z a A"],
  ["valueDesc", "Maior valor primeiro"],
  ["valueAsc", "Menor valor primeiro"],
];

const FORMATS: [string, string][] = [
  ["", "Padrão"],
  ["0", "Inteiro (1234)"],
  ["#,##0", "Milhares (1.234)"],
  ["#,##0.00", "Milhares com 2 casas"],
  ['"R$" #,##0.00', "Moeda (R$)"],
  ["0.0%", "Porcentagem (1 casa)"],
  ["0.00%", "Porcentagem (2 casas)"],
];

type Area = "filters" | "cols" | "rows" | "values";
const AREAS: [Area, string][] = [
  ["filters", "Filtros"],
  ["cols", "Colunas"],
  ["rows", "Linhas"],
  ["values", "Valores"],
];

interface Deps {
  engine: Engine;
  sheet: () => number;
  /** As células da seleção (`A1:B5`), ou o bloco de dados em volta da célula ativa. */
  sourceText: () => Promise<string>;
  /** Cria uma planilha, mostra-a e devolve o índice. */
  addSheet: () => Promise<number>;
  /** Avisa que algo mudou, para marcar o arquivo e redesenhar. */
  changed: () => Promise<void>;
  status: (text: string) => void;
}

const newField = (name: string): PivotField => ({
  name,
  group: "none",
  step: 10,
  sort: "labelAsc",
  sortValue: 0,
  top: null,
});

const newValue = (field: string, agg: PivotAgg = "sum"): PivotValue => ({
  field,
  agg,
  showAs: "normal",
  title: "",
  format: "",
  formula: "",
});

/** Liga o painel de campos da tabela dinâmica. */
export function setupPivotPanel(deps: Deps) {
  const { engine } = deps;
  const panel = document.querySelector<HTMLElement>("#pivot-panel") as HTMLElement;
  const menu = document.querySelector<HTMLElement>("#pivot-menu") as HTMLElement;

  let pivot: Pivot | null = null;
  let fields: string[] = [];
  /** Tabelas da planilha aberta, enviadas por quem desenha a grade. */
  let known: Pivot[] = [];
  /** Tabela que o usuário fechou; só reabre ao sair e voltar. */
  let dismissed: number | null = null;
  /** Item aberto para edição: área e posição. */
  let editing: { area: Area; index: number } | null = null;
  let timer: number | undefined;
  let saving = Promise.resolve();
  let message = "";
  const itemCache = new Map<string, string[]>();

  const el = <K extends keyof HTMLElementTagNameMap>(tag: K, cls = "", text = "") => {
    const node = document.createElement(tag);
    if (cls) node.className = cls;
    if (text) node.textContent = text;
    return node;
  };

  function select<T extends string>(options: [T, string][], value: T, onChange: (value: T) => void) {
    const node = el("select");
    for (const [key, label] of options) node.append(new Option(label, key, false, key === value));
    node.addEventListener("change", () => onChange(node.value as T));
    return node;
  }

  function labelled(text: string, input: HTMLElement) {
    const label = el("label");
    label.append(el("span", "", text), input);
    return label;
  }

  function textInput(value: string, placeholder: string, onInput: (value: string) => void) {
    const input = el("input");
    input.type = "text";
    input.value = value;
    input.placeholder = placeholder;
    input.autocomplete = "off";
    input.spellcheck = false;
    input.addEventListener("input", () => onInput(input.value));
    return input;
  }

  // -- Gravação ------------------------------------------------------------

  /** Redesenha a tabela com a definição atual, esperando o usuário parar de mexer. */
  function commit(immediate = false) {
    window.clearTimeout(timer);
    const run = () => {
      saving = saving.then(async () => {
        const current = pivot;
        if (!current) return;
        try {
          const done = await engine.updatePivot(current);
          // Só o tamanho da grade vem do motor; o resto é o que o usuário tem na tela.
          if (pivot) pivot = { ...pivot, width: done.width, height: done.height };
          message = "";
          await deps.changed();
        } catch (error) {
          message = String(error);
        }
        showMessage();
      });
    };
    if (immediate) run();
    else timer = window.setTimeout(run, 250);
  }

  function showMessage() {
    const box = panel.querySelector<HTMLElement>(".pvt-error");
    if (!box) return;
    box.textContent = message;
    box.hidden = message === "";
  }

  // -- Áreas ---------------------------------------------------------------

  const list = (area: Area): unknown[] => {
    if (!pivot) return [];
    return area === "filters" ? pivot.filters : area === "values" ? pivot.values : pivot[area];
  };

  function nameOf(area: Area, index: number): string {
    if (!pivot) return "";
    if (area === "filters") return pivot.filters[index].field;
    if (area === "values") {
      const value = pivot.values[index];
      if (value.title) return value.title;
      if (value.agg === "calc") return "Campo calculado";
      return `${AGGS.find(([key]) => key === value.agg)?.[1] ?? ""} de ${value.field}`;
    }
    return pivot[area][index].name;
  }

  /** Põe um campo numa área; linhas e colunas não repetem o campo, valores podem. */
  function addTo(area: Area, name: string, before?: number) {
    if (!pivot) return;
    if (area === "values") {
      pivot.values.splice(before ?? pivot.values.length, 0, newValue(name));
    } else if (area === "filters") {
      if (!pivot.filters.some((f) => f.field === name)) pivot.filters.push({ field: name, hidden: [] });
    } else {
      pivot.rows = pivot.rows.filter((f) => f.name !== name);
      pivot.cols = pivot.cols.filter((f) => f.name !== name);
      pivot[area].splice(before ?? pivot[area].length, 0, newField(name));
    }
    editing = null;
    render();
    commit();
  }

  function remove(area: Area, index: number) {
    if (!pivot) return;
    (list(area) as unknown[]).splice(index, 1);
    editing = null;
    render();
    commit();
  }

  /** Move um item de uma área para outra (ou dentro dela). */
  function move(from: { area: Area; index: number }, to: { area: Area; index?: number }) {
    if (!pivot) return;
    const name = from.area === "values" ? pivot.values[from.index].field : nameOf(from.area, from.index);
    if (from.area === to.area) {
      const items = list(from.area) as unknown[];
      const [item] = items.splice(from.index, 1);
      let at = to.index ?? items.length;
      if (to.index !== undefined && to.index > from.index) at -= 1;
      items.splice(at, 0, item);
      editing = null;
      render();
      commit();
      return;
    }
    // Um valor só sai da área de valores; os outros campos mantêm o nome.
    const fieldName = from.area === "values" ? name : name;
    remove(from.area, from.index);
    addTo(to.area, fieldName, to.index);
  }

  // -- Edição de um item ------------------------------------------------------

  function fieldEditor(area: "rows" | "cols", index: number) {
    const field = (pivot as Pivot)[area][index];
    const box = el("div", "pvt-edit");
    box.append(
      labelled(
        "Agrupar por",
        select(GROUPS, field.group, (value) => {
          field.group = value;
          render();
          commit();
        }),
      ),
    );
    if (field.group === "bucket") {
      const step = textInput(String(field.step), "Tamanho da faixa", (value) => {
        const number = Number(value.replace(",", "."));
        if (number > 0) {
          field.step = number;
          commit();
        }
      });
      box.append(labelled("Tamanho da faixa", step));
    }
    box.append(
      labelled(
        "Ordem",
        select(SORTS, field.sort, (value) => {
          field.sort = value;
          render();
          commit();
        }),
      ),
    );
    if (field.sort === "valueAsc" || field.sort === "valueDesc") {
      const choices: [string, string][] = (pivot as Pivot).values.map((_, i) => [String(i), nameOf("values", i)]);
      if (choices.length > 0) {
        box.append(
          labelled(
            "Pelo valor",
            select(choices, String(field.sortValue), (value) => {
              field.sortValue = Number(value);
              commit();
            }),
          ),
        );
      }
    }
    box.append(
      labelled(
        "Mostrar só os primeiros",
        textInput(field.top === null ? "" : String(field.top), "Todos", (value) => {
          const number = Math.floor(Number(value));
          field.top = number > 0 ? number : null;
          commit();
        }),
      ),
    );
    return box;
  }

  function valueEditor(index: number) {
    const value = (pivot as Pivot).values[index];
    const box = el("div", "pvt-edit");
    if (value.agg === "calc") {
      box.append(
        labelled(
          "Conta (campos entre colchetes)",
          textInput(value.formula, "[Preço] * [Quantidade]", (text) => {
            value.formula = text;
            commit();
          }),
        ),
      );
    } else {
      box.append(
        labelled(
          "Resumir por",
          select(AGGS, value.agg, (agg) => {
            value.agg = agg;
            render();
            commit();
          }),
        ),
      );
    }
    box.append(
      labelled(
        "Mostrar valor como",
        select(SHOW_AS, value.showAs, (showAs) => {
          value.showAs = showAs;
          commit();
        }),
      ),
      labelled(
        "Título",
        textInput(value.title, "Automático", (text) => {
          value.title = text;
          commit();
        }),
      ),
      labelled(
        "Formato",
        select(
          FORMATS.some(([key]) => key === value.format) ? FORMATS : [...FORMATS, [value.format, value.format]],
          value.format,
          (format) => {
            value.format = format;
            commit();
          },
        ),
      ),
    );
    return box;
  }

  function filterEditor(index: number) {
    const filter = (pivot as Pivot).filters[index];
    const box = el("div", "pvt-edit");
    const items = el("div", "pvt-items");
    box.append(items);
    const draw = (all: string[]) => {
      items.replaceChildren(
        ...all.map((item) => {
          const row = el("label", "pvt-check");
          const check = el("input");
          check.type = "checkbox";
          check.checked = !filter.hidden.includes(item);
          check.addEventListener("change", () => {
            filter.hidden = check.checked ? filter.hidden.filter((h) => h !== item) : [...filter.hidden, item];
            commit();
          });
          row.append(check, el("span", "", item));
          return row;
        }),
      );
    };
    const group = (pivot as Pivot).rows.concat((pivot as Pivot).cols).find((f) => f.name === filter.field);
    const key = `${(pivot as Pivot).sourceSheet}|${(pivot as Pivot).source}|${filter.field}|${group?.group}|${group?.step}`;
    const cached = itemCache.get(key);
    if (cached) draw(cached);
    else {
      void engine
        .pivotItems((pivot as Pivot).sourceSheet, (pivot as Pivot).source, group ?? newField(filter.field))
        .then((all) => {
          itemCache.set(key, all);
          draw(all);
        })
        .catch((error) => {
          items.textContent = String(error);
        });
    }
    const buttons = el("div", "pvt-buttons");
    const mark = (checked: boolean) => {
      const all = itemCache.get(key) ?? [];
      filter.hidden = checked ? [] : [...all];
      draw(all);
      commit();
    };
    const every = el("button", "", "Todos");
    every.type = "button";
    every.addEventListener("click", () => mark(true));
    const none = el("button", "", "Nenhum");
    none.type = "button";
    none.addEventListener("click", () => mark(false));
    buttons.append(every, none);
    box.append(buttons);
    return box;
  }

  // -- Desenho -------------------------------------------------------------

  let dragging: { area: Area; index: number } | { field: string } | null = null;

  function chip(area: Area, index: number) {
    const item = el("li", "pvt-chip");
    item.draggable = true;
    item.dataset.open = String(editing?.area === area && editing.index === index);
    const name = el("button", "pvt-name", nameOf(area, index));
    name.type = "button";
    name.title = "Clique para ajustar";
    name.addEventListener("click", () => {
      editing = editing?.area === area && editing.index === index ? null : { area, index };
      render();
    });
    const close = el("button", "pvt-x", "×");
    close.type = "button";
    close.setAttribute("aria-label", `Remover ${nameOf(area, index)}`);
    close.addEventListener("click", () => remove(area, index));
    item.append(name, close);
    item.addEventListener("dragstart", (event) => {
      dragging = { area, index };
      event.dataTransfer?.setData("text/plain", nameOf(area, index));
    });
    item.addEventListener("dragover", (event) => event.preventDefault());
    item.addEventListener("drop", (event) => {
      event.preventDefault();
      event.stopPropagation();
      drop(area, index);
    });
    const wrap = el("li", "pvt-item");
    wrap.append(item);
    if (editing?.area === area && editing.index === index) {
      wrap.append(
        area === "values" ? valueEditor(index) : area === "filters" ? filterEditor(index) : fieldEditor(area, index),
      );
    }
    return wrap;
  }

  function drop(area: Area, index?: number) {
    const source = dragging;
    dragging = null;
    if (!source) return;
    if ("field" in source) addTo(area, source.field, index);
    else move(source, { area, index });
  }

  function fieldMenu(name: string, at: DOMRect) {
    menu.replaceChildren(
      ...AREAS.slice()
        .reverse()
        .map(([area, label]) => {
          const item = el("li", "", `Adicionar a ${label.toLowerCase()}`);
          item.addEventListener("pointerdown", (event) => {
            event.preventDefault();
            menu.hidden = true;
            addTo(area, name);
          });
          return item;
        }),
    );
    menu.hidden = false;
    menu.style.left = `${Math.max(0, Math.min(at.left, window.innerWidth - menu.offsetWidth - 8))}px`;
    menu.style.top = `${Math.min(at.bottom + 2, window.innerHeight - menu.offsetHeight - 8)}px`;
  }

  function toggle(label: string, key: "grandRows" | "grandCols" | "subtotals" | "repeatLabels") {
    const row = el("label", "pvt-check");
    const check = el("input");
    check.type = "checkbox";
    check.checked = (pivot as Pivot)[key];
    check.addEventListener("change", () => {
      (pivot as Pivot)[key] = check.checked;
      commit();
    });
    row.append(check, el("span", "", label));
    return row;
  }

  function render() {
    if (!pivot) {
      panel.hidden = true;
      return;
    }
    const current = pivot;
    panel.hidden = false;
    const head = el("div", "pvt-head");
    head.append(el("strong", "", "Tabela dinâmica"));
    const close = el("button", "pvt-x", "×");
    close.type = "button";
    close.setAttribute("aria-label", "Fechar");
    close.addEventListener("click", () => {
      dismissed = current.id;
      pivot = null;
      panel.hidden = true;
      document.querySelector<HTMLElement>("#grid")?.focus();
    });
    head.append(close);

    const source = labelled(
      "Dados de origem",
      textInput(current.source, "A1:E100", (text) => {
        current.source = text.trim();
        window.clearTimeout(timer);
        timer = window.setTimeout(async () => {
          try {
            fields = await engine.pivotFields(current.sourceSheet, current.source);
            message = "";
            itemCache.clear();
            render();
            commit(true);
          } catch (error) {
            message = String(error);
            showMessage();
          }
        }, 500);
      }),
    );

    const fieldList = el("ul", "pvt-fields");
    for (const name of fields) {
      const item = el("li", "pvt-field", name);
      item.draggable = true;
      item.title = "Arraste para uma área, ou clique para escolher";
      item.addEventListener("dragstart", (event) => {
        dragging = { field: name };
        event.dataTransfer?.setData("text/plain", name);
      });
      item.addEventListener("click", () => fieldMenu(name, item.getBoundingClientRect()));
      fieldList.append(item);
    }

    const areas = el("div", "pvt-areas");
    for (const [area, label] of AREAS) {
      const box = el("div", "pvt-area");
      box.append(el("h4", "", label));
      const items = el("ul", "pvt-list");
      const count = (list(area) as unknown[]).length;
      for (let i = 0; i < count; i++) items.append(chip(area, i));
      if (count === 0) items.append(el("li", "pvt-hint", "Arraste campos para cá"));
      box.append(items);
      box.addEventListener("dragover", (event) => event.preventDefault());
      box.addEventListener("drop", (event) => {
        event.preventDefault();
        drop(area);
      });
      areas.append(box);
    }

    const calc = el("button", "", "+ Campo calculado");
    calc.type = "button";
    calc.addEventListener("click", () => {
      current.values.push(newValue("", "calc"));
      editing = { area: "values", index: current.values.length - 1 };
      render();
    });
    const refresh = el("button", "", "Atualizar");
    refresh.type = "button";
    refresh.addEventListener("click", () => commit(true));
    const del = el("button", "pvt-danger", "Excluir tabela");
    del.type = "button";
    del.addEventListener("click", () => void removePivot());
    const buttons = el("div", "pvt-buttons");
    buttons.append(calc, refresh, del);

    const options = el("div", "pvt-options");
    options.append(
      toggle("Total geral nas linhas", "grandRows"),
      toggle("Total geral nas colunas", "grandCols"),
      toggle("Subtotais dos grupos", "subtotals"),
      toggle("Repetir os nomes dos grupos", "repeatLabels"),
    );

    const error = el("p", "pvt-error");
    error.setAttribute("role", "alert");
    error.hidden = message === "";
    error.textContent = message;
    panel.replaceChildren(head, source, el("h4", "", "Campos"), fieldList, areas, options, error, buttons);
  }

  async function removePivot() {
    if (!pivot) return;
    const id = pivot.id;
    const clear = await engine.confirm("Excluir a tabela dinâmica e o que ela mostra na planilha?", "Excluir");
    if (!clear) return;
    try {
      await engine.deletePivot(id, true);
    } catch (error) {
      message = String(error);
      showMessage();
      return;
    }
    pivot = null;
    panel.hidden = true;
    await deps.changed();
  }

  // -- Entradas ------------------------------------------------------------

  async function open(found: Pivot) {
    pivot = JSON.parse(JSON.stringify(found)) as Pivot;
    editing = null;
    message = "";
    try {
      fields = await engine.pivotFields(pivot.sourceSheet, pivot.source);
    } catch (error) {
      fields = [];
      message = String(error);
    }
    render();
  }

  const contains = (item: Pivot, col: number, row: number) =>
    item.width > 0 && col >= item.col && col < item.col + item.width && row >= item.row && row < item.row + item.height;

  return {
    /** Cria uma tabela dinâmica numa planilha nova, com a seleção como origem. */
    async create() {
      let source: string;
      try {
        source = await deps.sourceText();
      } catch (error) {
        deps.status(`Erro: ${error}`);
        return;
      }
      const sourceSheet = deps.sheet();
      try {
        await engine.pivotFields(sourceSheet, source);
      } catch (error) {
        deps.status(`Não deu para usar essas células: ${error}`);
        return;
      }
      let target: number;
      try {
        target = await deps.addSheet();
        await engine.renameSheet(target, `Dinâmica ${known.length + 1}`).catch(() => undefined);
        const made = await engine.addPivot({
          id: 0,
          sourceSheet,
          source,
          sheet: target,
          col: 0,
          row: 0,
          rows: [],
          cols: [],
          values: [],
          filters: [],
          grandRows: true,
          grandCols: true,
          subtotals: true,
          repeatLabels: false,
          width: 0,
          height: 0,
        });
        dismissed = null;
        await deps.changed();
        await open(made);
      } catch (error) {
        deps.status(`Erro ao criar a tabela dinâmica: ${error}`);
      }
    },

    /** Recebe as tabelas da planilha aberta. */
    setKnown(list: Pivot[]) {
      known = list;
      // Mantém a largura e a altura reais da grade na definição aberta.
      if (pivot) {
        const same = list.find((item) => item.id === pivot?.id);
        if (same) pivot = { ...pivot, width: same.width, height: same.height };
      }
    },

    /** Abre o painel quando a célula ativa está numa tabela, e o esconde ao sair dela. */
    sync(col: number, row: number) {
      const found = known.find((item) => contains(item, col, row));
      if (!found) {
        dismissed = null;
        if (pivot && !panel.hidden && !panel.contains(document.activeElement)) {
          pivot = null;
          panel.hidden = true;
        }
        return;
      }
      if (pivot?.id === found.id || dismissed === found.id) return;
      void open(found);
    },

    closeMenu() {
      menu.hidden = true;
    },
  };
}
