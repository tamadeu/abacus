import { invoke, isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ask, message, open, save } from "@tauri-apps/plugin-dialog";

/** Parte do estilo de uma célula que a grade sabe desenhar. */
export interface CellStyle {
  bold: boolean;
  italic: boolean;
  underline: boolean;
  strike: boolean;
  /** Cor do texto, como `#RRGGBB`. */
  color: string | null;
  /** Cor de fundo, como `#RRGGBB`. */
  fill: string | null;
  /** Sem alinhamento explícito, números ficam à direita e texto à esquerda. */
  align: "left" | "center" | "right" | null;
  /** Alinhamento vertical. Sem ele, o conteúdo fica embaixo, como no Excel. */
  valign: "top" | "center" | null;
  /** Tamanho da fonte em porcentagem do padrão (200 é o dobro); `null` é o padrão. */
  size: number | null;
  /** O texto quebra em várias linhas dentro da célula. */
  wrap: boolean;
  /** Bordas da célula, na ordem: cima, direita, baixo, esquerda. */
  borders: [BorderLine | null, BorderLine | null, BorderLine | null, BorderLine | null];
}

/** Uma borda de célula. */
export interface BorderLine {
  /** Cor, como `#RRGGBB`. */
  color: string;
  /** Espessura em pixels: 1, 2 ou 3. */
  width: number;
  /** Tracejada ou pontilhada. */
  dashed: boolean;
}

/** Quais bordas de um intervalo desenhar; `none` remove todas. */
export type BorderKind = "all" | "outer" | "top" | "bottom" | "left" | "right" | "none";

/** Inserção ou exclusão de linhas ou colunas inteiras. */
export type Structure = "insertRows" | "deleteRows" | "insertCols" | "deleteCols";

/** O que a interface precisa saber para desenhar uma célula. */
export interface CellView {
  col: number;
  row: number;
  /** Conteúdo editável (o que aparece na barra de fórmulas). */
  input: string;
  /** Texto mostrado na grade, já formatado. */
  display: string;
  kind: "number" | "text" | "error";
  /** Ausente quando a célula usa o estilo padrão. */
  style?: CellStyle;
  /** Barra de dados da formatação condicional, se a célula tem uma. */
  bar?: DataBar;
}

/** Barra de dados: onde começa e termina, de 0 a 1 da largura da célula. */
export interface DataBar {
  value: number;
  axis: number;
  color: string;
}

/** O que uma regra de formatação condicional testa. */
export type CondKind =
  | "greater"
  | "greaterEqual"
  | "less"
  | "lessEqual"
  | "equal"
  | "notEqual"
  | "between"
  | "notBetween"
  | "contains"
  | "notContains"
  | "beginsWith"
  | "endsWith"
  | "duplicates"
  | "unique"
  | "blanks"
  | "notBlanks"
  | "errors"
  | "aboveAverage"
  | "belowAverage"
  | "top"
  | "bottom"
  | "colorScale"
  | "dataBar"
  | "other";

export type ValidationKind = "list" | "whole" | "decimal" | "textLength";
export type ValidationOp =
  | "between"
  | "notBetween"
  | "equal"
  | "notEqual"
  | "greater"
  | "greaterEqual"
  | "less"
  | "lessEqual";

export type PivotGroup = "none" | "year" | "quarter" | "month" | "yearMonth" | "bucket";
export type PivotSort = "labelAsc" | "labelDesc" | "valueAsc" | "valueDesc";
export type PivotAgg =
  | "sum"
  | "count"
  | "countNums"
  | "average"
  | "min"
  | "max"
  | "product"
  | "stdDev"
  | "var"
  | "distinct"
  | "calc";
export type PivotShowAs = "normal" | "pctTotal" | "pctRow" | "pctCol";

/** Um campo nas linhas ou nas colunas de uma tabela dinâmica. */
export interface PivotField {
  name: string;
  group: PivotGroup;
  step: number;
  sort: PivotSort;
  sortValue: number;
  top: number | null;
}

/** Um campo da área de valores. */
export interface PivotValue {
  field: string;
  agg: PivotAgg;
  showAs: PivotShowAs;
  title: string;
  format: string;
  formula: string;
}

/** Uma tabela dinâmica. */
export interface Pivot {
  /** 0 numa tabela nova. */
  id: number;
  sourceSheet: number;
  /** Células de origem (`A1:E100`), com os nomes dos campos na primeira linha. */
  source: string;
  sheet: number;
  col: number;
  row: number;
  rows: PivotField[];
  cols: PivotField[];
  values: PivotValue[];
  filters: { field: string; hidden: string[] }[];
  grandRows: boolean;
  grandCols: boolean;
  subtotals: boolean;
  repeatLabels: boolean;
  /** Tamanho da grade escrita; o motor preenche. */
  width: number;
  height: number;
  /** Parte da grade que um gráfico usa (sem totais); o motor preenche. */
  chartArea?: Range;
}

/** Uma regra de validação de dados. */
export interface Validation {
  /** 0 numa regra nova. */
  id: number;
  sheet: number;
  /** Células da regra, como `A1:B5`. */
  range: string;
  kind: ValidationKind;
  operator: ValidationOp;
  /** O limite, ou a lista (`Sim; Não` ou `=A1:A5`). */
  value: string;
  value2: string;
  /** Aviso mostrado ao recusar; vazio usa o padrão. */
  message: string;
}

/** Uma regra de formatação condicional. */
export interface CondRule {
  /** Posição da regra na planilha; `null` numa regra nova. */
  index: number | null;
  /** Células da regra, como `A1:B5`. */
  range: string;
  kind: CondKind;
  value: string;
  value2: string;
  /** Fundo; cor da barra; ou, numa escala de cor, a do maior valor. */
  fill: string | null;
  /** Texto; ou, numa escala de cor, a do menor valor. */
  color: string | null;
  /** Cor do meio de uma escala de três cores. */
  mid: string | null;
  bold: boolean;
}

/** O pedaço de uma planilha que cabe na tela. */
/** Contas sobre as células de uma seleção, para a barra de status. */
export interface Stats {
  /** Células com algum conteúdo. */
  count: number;
  /** Células cujo valor é um número (datas incluídas). */
  numbers: number;
  sum: number;
}

/** Tipo de um gráfico. */
export type ChartKind = "column" | "bar" | "line" | "pie" | "area" | "scatter" | "donut";
export type Stacking = "none" | "stacked" | "percent";
export type LegendPos = "auto" | "none" | "bottom" | "right" | "top" | "left";
export type TrendKind = "linear" | "poly" | "exp" | "log" | "moving";

/** Linha de tendência de uma série. */
export interface Trend {
  kind: TrendKind;
  /** Grau do polinômio (2 a 6). */
  order: number;
  /** Pontos da média móvel. */
  period: number;
  equation: boolean;
  r2: boolean;
}

/** Aparência e extras de uma série (na ordem em que saem do intervalo). */
export interface SeriesStyle {
  /** `#RRGGBB`; nulo usa a paleta. */
  color: string | null;
  /** Em colunas, a série vira linha (combinado). */
  line: boolean;
  /** Usa o eixo de valores da direita. */
  secondary: boolean;
  trend: Trend | null;
}

/** O que o gráfico mostra, lido das células. */
export interface ChartData {
  categories: string[];
  /** Valor de X de cada categoria (1, 2, 3… ou, na dispersão, o número da primeira coluna). */
  x: (number | null)[];
  series: { name: string; values: (number | null)[]; trend?: { points: [number, number][]; label: string } }[];
}

/**
 * Um gráfico sobre a grade, do canto da célula `from` ao fim da célula `to`
 * (inclusive), com os dados do intervalo `range`.
 */
export interface ChartInfo {
  id: number;
  sheet: number;
  kind: ChartKind;
  title: string;
  range: Range;
  fromCol: number;
  fromRow: number;
  toCol: number;
  toRow: number;
  stacking: Stacking;
  legend: LegendPos;
  /** Mostra os valores (ou os percentuais, na pizza) sobre os dados. */
  labels: boolean;
  xTitle: string;
  yTitle: string;
  yMin: number | null;
  yMax: number | null;
  grid: boolean;
  smooth: boolean;
  styles: SeriesStyle[];
  /** Tabela dinâmica que alimenta o gráfico. */
  pivot?: number;
  data: ChartData;
}

/** Direção de um salto até a borda de um bloco de dados (Ctrl+seta). */
export type Direction = "left" | "right" | "up" | "down";

/** O que fazer com as alterações não salvas. */
export type SaveChoice = "save" | "discard" | "cancel";

export interface View {
  /** Quantas colunas, a partir da primeira da planilha, ficam congeladas à esquerda. */
  frozenCols: number;
  /** Quantas linhas ficam congeladas no topo. */
  frozenRows: number;
  /**
   * Largura, em pixels, de cada coluna da tela: primeiro as congeladas,
   * depois as visíveis a partir da primeira pedida. Uma coluna oculta tem
   * largura zero.
   */
  colWidths: number[];
  /** Altura, em pixels, de cada linha da tela, na mesma ordem. */
  rowHeights: number[];
  /**
   * Células com conteúdo ou estilo, já calculadas. Inclui a célula principal
   * de cada área mesclada da tela, mesmo que ela esteja fora.
   */
  cells: CellView[];
  /** Áreas mescladas que aparecem na tela. */
  merges: Range[];
}

/** Um retângulo de células, com coluna e linha iniciais a partir de zero. */
export interface Range {
  col: number;
  row: number;
  cols: number;
  rows: number;
}

/** Posição de uma célula, a partir de zero. */
export interface CellPos {
  col: number;
  row: number;
}

/** O que dá para filtrar numa coluna de uma tabela. */
export interface FilterOptions {
  /** Linhas de dados da tabela, sem o cabeçalho. */
  range: Range;
  /** Coluna filtrada. */
  col: number;
  /** Valores distintos da coluna, em ordem. */
  values: FilterValue[];
}

export interface FilterValue {
  /** Texto mostrado na célula; vazio para as células em branco. */
  text: string;
  /** Há pelo menos uma linha com este valor à mostra. */
  shown: boolean;
}

/**
 * Propriedades de estilo que a interface pode alterar. `bold`, `italic`,
 * `underline` e `strike` recebem `"true"` ou `"false"`; `color` e `fill`,
 * uma cor `#RRGGBB` ou vazio para remover; `align`, `"left"`, `"center"`,
 * `"right"` ou `"general"`; `format`, um formato numérico do Excel; `wrap`,
 * `"true"` ou `"false"`; `size`, a variação em pontos (`"1"` ou `"-1"`);
 * `valign`, `"top"`, `"center"` ou `"bottom"`.
 */
export type StyleProperty =
  | "bold"
  | "italic"
  | "underline"
  | "strike"
  | "color"
  | "fill"
  | "align"
  | "format"
  | "wrap"
  | "size"
  | "valign";

/** Uma planilha visível da pasta de trabalho. */
export interface SheetInfo {
  /** Índice usado nas demais chamadas. */
  index: number;
  name: string;
}

/** Abrir e salvar arquivos, com os diálogos do sistema. */
export interface FileAccess {
  /**
   * Pergunta qual arquivo abrir (`.xlsx` ou `.csv`) e o carrega. Devolve o
   * caminho, ou `null` se a pessoa cancelou.
   */
  open(): Promise<string | null>;
  /** O arquivo que a linha de comando mandou abrir, se houver (uma vez só). */
  startup?(): Promise<string | null>;
  /**
   * Salva em `path`; sem caminho, pergunta onde. Se o caminho termina em
   * `.csv`, grava só a planilha `sheet`, como texto. Devolve o caminho usado,
   * ou `null` se a pessoa cancelou.
   */
  save(path: string | null, sheet: number): Promise<string | null>;
  /** Pergunta onde gravar uma imagem PNG e a grava. Devolve o caminho, ou `null` se cancelou. */
  saveImage?(png: Uint8Array, name: string): Promise<string | null>;
}

/**
 * Ponte entre a interface e o motor de planilhas. A interface só conhece
 * esta interface, então trocar a casca (Tauri hoje, WebAssembly na versão
 * web) não muda o resto do código.
 */
export interface Engine {
  /** Aviso fixo para mostrar no cabeçalho, se houver. */
  readonly notice: string | null;
  /** `null` quando a casca não tem acesso a arquivos. */
  readonly files: FileAccess | null;
  /** Se `format` tem efeito. */
  readonly formatting: boolean;
  /** Se dá para congelar, ocultar, classificar e filtrar. */
  readonly tables: boolean;
  /** Pede confirmação para uma ação que não dá para desfazer sem perda. `okLabel` é o texto do botão que confirma. */
  confirm(message: string, okLabel: string): Promise<boolean>;
  /** Pergunta se o documento `name`, com alterações não salvas, deve ser salvo. */
  askToSave(name: string): Promise<SaveChoice>;
  /** Registra o que fazer quando a janela for fechada: `handler` devolve se pode fechar. */
  onClose(handler: () => Promise<boolean>): void;
  sheets(): Promise<SheetInfo[]>;
  /** Contagem e soma das células da seleção. */
  stats(sheet: number, range: Range): Promise<Stats>;
  /** Para onde vai a célula ativa com Ctrl+seta. */
  jump(sheet: number, col: number, row: number, direction: Direction): Promise<CellPos>;
  /** A última célula da planilha que tem algo, de baixo e à direita. */
  lastCell(sheet: number): Promise<CellPos>;
  /** Cria uma planilha vazia e devolve o índice dela. */
  addSheet(): Promise<number>;
  renameSheet(sheet: number, name: string): Promise<void>;
  /** Exclui uma planilha. Os índices das planilhas seguintes mudam. */
  deleteSheet(sheet: number): Promise<void>;
  /** Define a largura de uma coluna, em pixels. */
  setColWidth(sheet: number, col: number, width: number): Promise<void>;
  /** Define a altura de uma linha, em pixels. */
  setRowHeight(sheet: number, row: number, height: number): Promise<void>;
  setCell(sheet: number, col: number, row: number, input: string): Promise<void>;
  /** O que cabe numa tela de `width` por `height` pixels a partir da célula dada. */
  view(sheet: number, firstCol: number, firstRow: number, width: number, height: number): Promise<View>;
  /** Altera uma propriedade de estilo nas células do intervalo. */
  format(sheet: number, range: Range, property: StyleProperty, value: string): Promise<void>;
  /** Apaga o conteúdo das células do intervalo, mantendo o estilo. */
  clearCells(sheet: number, range: Range): Promise<void>;
  /** Mescla o intervalo numa célula só, centralizada. */
  merge(sheet: number, range: Range): Promise<void>;
  /** Desfaz as áreas mescladas que tocam o intervalo. */
  unmerge(sheet: number, range: Range): Promise<void>;
  /** Desenha (ou remove) bordas finas no intervalo. */
  setBorder(sheet: number, range: Range, kind: BorderKind): Promise<void>;
  /** Insere ou exclui `count` linhas ou colunas a partir de `index`. */
  changeStructure(sheet: number, change: Structure, index: number, count: number): Promise<void>;
  /**
   * Preenche a partir do intervalo até a linha ou coluna `to`, como ao
   * arrastar a alça da seleção.
   */
  fill(sheet: number, range: Range, axis: "rows" | "cols", to: number): Promise<void>;
  /** Copia o intervalo para a área de transferência. Com `cut`, colar apaga a origem. */
  copy(sheet: number, range: Range, cut: boolean): Promise<void>;
  /** Cola a partir da célula dada. Devolve o intervalo preenchido, ou `null` se não havia o que colar. */
  paste(sheet: number, col: number, row: number): Promise<Range | null>;
  /** Congela as primeiras `cols` colunas e `rows` linhas; zero descongela. */
  setFrozen(sheet: number, cols: number, rows: number): Promise<void>;
  /** Oculta ou reexibe `count` linhas ou colunas a partir de `first`. */
  setHidden(sheet: number, axis: "rows" | "cols", first: number, count: number, hidden: boolean): Promise<void>;
  /**
   * Células da planilha que contêm o texto, sem diferenciar maiúsculas de
   * minúsculas, em ordem de leitura.
   */
  find(sheet: number, query: string): Promise<CellPos[]>;
  /** Troca o texto procurado numa célula. Devolve `false` se o texto só existe no valor calculado. */
  replace(sheet: number, col: number, row: number, query: string, replacement: string): Promise<boolean>;
  /** Troca o texto na planilha inteira. Devolve quantas células mudaram. */
  replaceAll(sheet: number, query: string, replacement: string): Promise<number>;
  /**
   * Classifica linhas pela coluna `keyCol`: a seleção, ou a tabela em volta
   * dela se for uma célula só. Devolve o intervalo classificado.
   */
  sort(sheet: number, range: Range, keyCol: number, descending: boolean): Promise<Range>;
  /** Valores que dá para filtrar na coluna da célula, na tabela em volta dela. */
  filterOptions(sheet: number, col: number, row: number): Promise<FilterOptions>;
  /** Mostra só as linhas do intervalo com um dos valores na coluna. Devolve quantas ficaram ocultas. */
  applyFilter(sheet: number, range: Range, col: number, allowed: string[]): Promise<number>;
  /** Regras de formatação condicional da planilha, da de maior prioridade para a de menor. */
  pivots(sheet: number): Promise<Pivot[]>;
  pivotFields(sheet: number, source: string): Promise<string[]>;
  pivotItems(sheet: number, source: string, field: PivotField): Promise<string[]>;
  addPivot(pivot: Pivot): Promise<Pivot>;
  updatePivot(pivot: Pivot): Promise<Pivot>;
  refreshPivot(id: number): Promise<Pivot>;
  deletePivot(id: number, clear: boolean): Promise<void>;
  refreshAllPivots(): Promise<number>;
  validations(sheet: number): Promise<Validation[]>;
  addValidation(sheet: number, rule: Validation): Promise<Validation>;
  updateValidation(sheet: number, rule: Validation): Promise<Validation>;
  deleteValidation(id: number): Promise<void>;
  /** Itens da lista suspensa da célula, ou `null` se ela não tem uma. */
  listOptions(sheet: number, col: number, row: number): Promise<string[] | null>;
  /** Mostra um aviso ao usuário. */
  alert(message: string): Promise<void>;
  condRules(sheet: number): Promise<CondRule[]>;
  addCondRule(sheet: number, rule: CondRule): Promise<void>;
  updateCondRule(sheet: number, rule: CondRule): Promise<void>;
  deleteCondRule(sheet: number, index: number): Promise<void>;
  /** O bloco de dados contínuo em volta da célula. */
  dataRegion(sheet: number, col: number, row: number): Promise<Range>;
  /** Gráficos da planilha. */
  charts(sheet: number): Promise<ChartInfo[]>;
  /** Cria um gráfico com os dados do intervalo, ao lado dele. */
  addChart(sheet: number, kind: ChartKind, range: Range): Promise<ChartInfo>;
  /** Gráfico com os dados de uma tabela dinâmica, que o acompanha. */
  addPivotChart(id: number, kind: ChartKind): Promise<ChartInfo>;
  /** Grava o novo estado de um gráfico (tipo, título, dados ou posição). */
  updateChart(chart: ChartInfo): Promise<ChartInfo>;
  deleteChart(id: number): Promise<void>;
  undo(): Promise<void>;
  redo(): Promise<void>;
}

const XLSX_FILTER = { name: "Pasta de trabalho do Excel", extensions: ["xlsx"] };
const CSV_FILTER = { name: "Texto separado por ponto e vírgula (CSV)", extensions: ["csv"] };
const OPEN_FILTER = { name: "Planilhas", extensions: ["xlsx", "csv"] };

/** Motor IronCalc, em Rust, chamado pelos comandos do Tauri. */
class TauriEngine implements Engine {
  readonly notice = null;
  readonly formatting = true;
  readonly tables = true;

  readonly files: FileAccess = {
    async open() {
      const path = await open({ multiple: false, directory: false, filters: [OPEN_FILTER, XLSX_FILTER, CSV_FILTER] });
      if (path === null) return null;
      await invoke("open_file", { path });
      return path;
    },
    async startup() {
      return invoke<string | null>("startup_file");
    },
    async save(path, sheet) {
      let target = path ?? (await save({ filters: [XLSX_FILTER, CSV_FILTER], defaultPath: "Sem título.xlsx" }));
      if (target === null) return null;
      if (target.toLowerCase().endsWith(".csv")) {
        await invoke("export_csv", { sheet, path: target });
        return target;
      }
      if (!target.toLowerCase().endsWith(".xlsx")) target += ".xlsx";
      await invoke("save_file", { path: target });
      return target;
    },
    async saveImage(png, name) {
      let target = await save({ filters: [{ name: "Imagem PNG", extensions: ["png"] }], defaultPath: `${name}.png` });
      if (target === null) return null;
      if (!target.toLowerCase().endsWith(".png")) target += ".png";
      await invoke("write_png", { path: target, bytes: Array.from(png) });
      return target;
    },
  };

  confirm(message: string, okLabel: string): Promise<boolean> {
    return ask(message, { title: "Abacus", kind: "warning", okLabel, cancelLabel: "Cancelar" });
  }

  stats(sheet: number, range: Range): Promise<Stats> {
    return invoke("selection_stats", { sheet, range });
  }

  jump(sheet: number, col: number, row: number, direction: Direction): Promise<CellPos> {
    return invoke("jump_cell", { sheet, col, row, direction });
  }

  lastCell(sheet: number): Promise<CellPos> {
    return invoke("last_cell", { sheet });
  }

  async askToSave(name: string): Promise<SaveChoice> {
    const labels = { yes: "Salvar", no: "Não salvar", cancel: "Cancelar" };
    const answer = await message(`Salvar as alterações em "${name}"?`, {
      title: "Abacus",
      kind: "warning",
      buttons: labels,
    });
    if (answer === labels.yes || answer === "Yes") return "save";
    if (answer === labels.no || answer === "No") return "discard";
    return "cancel";
  }

  onClose(handler: () => Promise<boolean>): void {
    // Se o `handler` não impedir, a janela se fecha sozinha depois dele.
    void getCurrentWindow().onCloseRequested(async (event) => {
      if (!(await handler())) event.preventDefault();
    });
  }

  renameSheet(sheet: number, name: string): Promise<void> {
    return invoke("rename_sheet", { sheet, name });
  }

  deleteSheet(sheet: number): Promise<void> {
    return invoke("delete_sheet", { sheet });
  }

  setColWidth(sheet: number, col: number, width: number): Promise<void> {
    return invoke("set_col_width", { sheet, col, width });
  }

  setRowHeight(sheet: number, row: number, height: number): Promise<void> {
    return invoke("set_row_height", { sheet, row, height });
  }

  sheets(): Promise<SheetInfo[]> {
    return invoke("get_sheets");
  }

  addSheet(): Promise<number> {
    return invoke("add_sheet");
  }

  setCell(sheet: number, col: number, row: number, input: string): Promise<void> {
    return invoke("set_cell", { sheet, col, row, input });
  }

  view(sheet: number, firstCol: number, firstRow: number, width: number, height: number): Promise<View> {
    return invoke("get_view", { sheet, firstCol, firstRow, width, height });
  }

  format(sheet: number, range: Range, property: StyleProperty, value: string): Promise<void> {
    return invoke("format_cells", { sheet, range, property, value });
  }

  clearCells(sheet: number, range: Range): Promise<void> {
    return invoke("clear_cells", { sheet, range });
  }

  merge(sheet: number, range: Range): Promise<void> {
    return invoke("merge_cells", { sheet, range });
  }

  unmerge(sheet: number, range: Range): Promise<void> {
    return invoke("unmerge_cells", { sheet, range });
  }

  setBorder(sheet: number, range: Range, kind: BorderKind): Promise<void> {
    return invoke("set_border", { sheet, range, kind });
  }

  changeStructure(sheet: number, change: Structure, index: number, count: number): Promise<void> {
    return invoke("change_structure", { sheet, change, index, count });
  }

  fill(sheet: number, range: Range, axis: "rows" | "cols", to: number): Promise<void> {
    return invoke("fill_cells", { sheet, range, axis, to });
  }

  copy(sheet: number, range: Range, cut: boolean): Promise<void> {
    return invoke("copy_cells", { sheet, range, cut });
  }

  paste(sheet: number, col: number, row: number): Promise<Range | null> {
    return invoke("paste_cells", { sheet, col, row });
  }

  setFrozen(sheet: number, cols: number, rows: number): Promise<void> {
    return invoke("set_frozen", { sheet, cols, rows });
  }

  setHidden(sheet: number, axis: "rows" | "cols", first: number, count: number, hidden: boolean): Promise<void> {
    return invoke("set_hidden", { sheet, axis, first, count, hidden });
  }

  find(sheet: number, query: string): Promise<CellPos[]> {
    return invoke("find_cells", { sheet, query });
  }

  replace(sheet: number, col: number, row: number, query: string, replacement: string): Promise<boolean> {
    return invoke("replace_cell", { sheet, col, row, query, replacement });
  }

  replaceAll(sheet: number, query: string, replacement: string): Promise<number> {
    return invoke("replace_all", { sheet, query, replacement });
  }

  sort(sheet: number, range: Range, keyCol: number, descending: boolean): Promise<Range> {
    return invoke("sort_cells", { sheet, range, keyCol, descending });
  }

  filterOptions(sheet: number, col: number, row: number): Promise<FilterOptions> {
    return invoke("filter_options", { sheet, col, row });
  }

  applyFilter(sheet: number, range: Range, col: number, allowed: string[]): Promise<number> {
    return invoke("apply_filter", { sheet, range, col, allowed });
  }

  pivots(sheet: number): Promise<Pivot[]> {
    return invoke("list_pivots", { sheet });
  }

  pivotFields(sheet: number, source: string): Promise<string[]> {
    return invoke("pivot_fields", { sheet, source });
  }

  pivotItems(sheet: number, source: string, field: PivotField): Promise<string[]> {
    return invoke("pivot_items", { sheet, source, field });
  }

  addPivot(pivot: Pivot): Promise<Pivot> {
    return invoke("add_pivot", { pivot });
  }

  updatePivot(pivot: Pivot): Promise<Pivot> {
    return invoke("update_pivot", { pivot });
  }

  refreshPivot(id: number): Promise<Pivot> {
    return invoke("refresh_pivot", { id });
  }

  deletePivot(id: number, clear: boolean): Promise<void> {
    return invoke("delete_pivot", { id, clear });
  }

  refreshAllPivots(): Promise<number> {
    return invoke("refresh_all_pivots");
  }

  validations(sheet: number): Promise<Validation[]> {
    return invoke("list_validations", { sheet });
  }

  addValidation(sheet: number, rule: Validation): Promise<Validation> {
    return invoke("add_validation", { sheet, rule });
  }

  updateValidation(sheet: number, rule: Validation): Promise<Validation> {
    return invoke("update_validation", { sheet, rule });
  }

  deleteValidation(id: number): Promise<void> {
    return invoke("delete_validation", { id });
  }

  listOptions(sheet: number, col: number, row: number): Promise<string[] | null> {
    return invoke("list_options", { sheet, col, row });
  }

  async alert(text: string): Promise<void> {
    await message(text, { title: "Abacus", kind: "warning" });
  }

  condRules(sheet: number): Promise<CondRule[]> {
    return invoke("list_cond_rules", { sheet });
  }

  addCondRule(sheet: number, rule: CondRule): Promise<void> {
    return invoke("add_cond_rule", { sheet, rule });
  }

  updateCondRule(sheet: number, rule: CondRule): Promise<void> {
    return invoke("update_cond_rule", { sheet, rule });
  }

  deleteCondRule(sheet: number, index: number): Promise<void> {
    return invoke("delete_cond_rule", { sheet, index });
  }

  dataRegion(sheet: number, col: number, row: number): Promise<Range> {
    return invoke("data_region", { sheet, col, row });
  }

  charts(sheet: number): Promise<ChartInfo[]> {
    return invoke("list_charts", { sheet });
  }

  addChart(sheet: number, kind: ChartKind, range: Range): Promise<ChartInfo> {
    return invoke("add_chart", { sheet, kind, range });
  }

  addPivotChart(id: number, kind: ChartKind): Promise<ChartInfo> {
    return invoke("add_pivot_chart", { id, kind });
  }

  updateChart(chart: ChartInfo): Promise<ChartInfo> {
    return invoke("update_chart", { chart });
  }

  deleteChart(id: number): Promise<void> {
    return invoke("delete_chart", { id });
  }

  undo(): Promise<void> {
    return invoke("undo");
  }

  redo(): Promise<void> {
    return invoke("redo");
  }
}

/**
 * Substituto usado quando a interface roda no navegador (`npm run dev`),
 * fora do Tauri. Guarda o que foi digitado, mas não calcula fórmulas, não
 * formata, não mescla, não redimensiona, não insere nem exclui linhas e
 * colunas, não congela, não oculta, não classifica, não filtra, não desfaz
 * edições e não abre nem salva arquivos.
 * Copiar e colar funcionam só dentro da própria página, e preencher com a
 * alça repete os valores, sem continuar sequências.
 */
class PreviewEngine implements Engine {
  readonly notice = "Pré-visualização no navegador (sem motor de cálculo)";
  readonly files = null;
  readonly formatting = false;
  readonly tables = false;
  private names = ["Planilha1"];
  private cells = new Map<string, CellView>();
  /** Última cópia: o texto de cada célula e, se foi um recortar, de onde saiu. */
  private copied: { rows: string[][]; cutFrom: { sheet: number; range: Range } | null } | null = null;

  async sheets(): Promise<SheetInfo[]> {
    return this.names.map((name, index) => ({ index, name }));
  }

  async confirm(message: string): Promise<boolean> {
    return window.confirm(message);
  }

  async askToSave(): Promise<SaveChoice> {
    return "discard";
  }

  async stats(sheet: number, range: Range): Promise<Stats> {
    const stats: Stats = { count: 0, numbers: 0, sum: 0 };
    for (const [key, cell] of this.cells) {
      if (!key.startsWith(`${sheet}:`)) continue;
      const inside = cell.col >= range.col && cell.col < range.col + range.cols && cell.row >= range.row && cell.row < range.row + range.rows;
      if (!inside || cell.input === "") continue;
      stats.count++;
      if (cell.kind === "number") {
        stats.numbers++;
        stats.sum += Number(cell.display.replace(",", "."));
      }
    }
    return stats;
  }

  async jump(_sheet: number, col: number, row: number): Promise<CellPos> {
    return { col, row };
  }

  async lastCell(): Promise<CellPos> {
    return { col: 0, row: 0 };
  }

  onClose(): void {}

  async addSheet(): Promise<number> {
    this.names.push(`Planilha${this.names.length + 1}`);
    return this.names.length - 1;
  }

  async renameSheet(sheet: number, name: string): Promise<void> {
    this.names[sheet] = name;
  }

  async deleteSheet(sheet: number): Promise<void> {
    this.names.splice(sheet, 1);
    // Remove as células da planilha e renumera as das seguintes.
    const kept = new Map<string, CellView>();
    for (const [key, cell] of this.cells) {
      const index = Number(key.split(":")[0]);
      if (index === sheet) continue;
      kept.set(`${index > sheet ? index - 1 : index}:${cell.col},${cell.row}`, cell);
    }
    this.cells = kept;
  }

  async setColWidth(): Promise<void> {}

  async setRowHeight(): Promise<void> {}

  async setCell(sheet: number, col: number, row: number, input: string): Promise<void> {
    const key = `${sheet}:${col},${row}`;
    if (input.trim() === "") {
      this.cells.delete(key);
      return;
    }
    const isNumber = /^-?\d+([.,]\d+)?$/.test(input.trim());
    this.cells.set(key, { col, row, input, display: input, kind: isNumber ? "number" : "text" });
  }

  async view(sheet: number, firstCol: number, firstRow: number, width: number, height: number): Promise<View> {
    // Mesmos tamanhos padrão do IronCalc.
    const cols = Math.ceil(width / 90);
    const rows = Math.ceil(height / 25);
    const range = { col: firstCol, row: firstRow, cols, rows };
    return {
      frozenCols: 0,
      frozenRows: 0,
      colWidths: new Array<number>(cols).fill(90),
      rowHeights: new Array<number>(rows).fill(25),
      cells: this.inRange(sheet, range).map(([, cell]) => cell),
      merges: [],
    };
  }

  async format(): Promise<void> {}

  async merge(): Promise<void> {}

  async unmerge(): Promise<void> {}

  async setBorder(): Promise<void> {}

  async changeStructure(): Promise<void> {}

  async clearCells(sheet: number, range: Range): Promise<void> {
    for (const [key] of this.inRange(sheet, range)) this.cells.delete(key);
  }

  async fill(sheet: number, range: Range, axis: "rows" | "cols", to: number): Promise<void> {
    const input = (col: number, row: number) => this.cells.get(`${sheet}:${col},${row}`)?.input ?? "";
    // Posição dentro da origem, repetida em ciclo.
    const wrapIn = (value: number, start: number, size: number) => start + ((((value - start) % size) + size) % size);
    if (axis === "rows") {
      const step = to >= range.row ? 1 : -1;
      for (let row = step > 0 ? range.row + range.rows : range.row - 1; step > 0 ? row <= to : row >= to; row += step) {
        for (let col = range.col; col < range.col + range.cols; col++) {
          await this.setCell(sheet, col, row, input(col, wrapIn(row, range.row, range.rows)));
        }
      }
    } else {
      const step = to >= range.col ? 1 : -1;
      for (let col = step > 0 ? range.col + range.cols : range.col - 1; step > 0 ? col <= to : col >= to; col += step) {
        for (let row = range.row; row < range.row + range.rows; row++) {
          await this.setCell(sheet, col, row, input(wrapIn(col, range.col, range.cols), row));
        }
      }
    }
  }

  async copy(sheet: number, range: Range, cut: boolean): Promise<void> {
    const rows: string[][] = [];
    for (let row = range.row; row < range.row + range.rows; row++) {
      const line: string[] = [];
      for (let col = range.col; col < range.col + range.cols; col++) {
        line.push(this.cells.get(`${sheet}:${col},${row}`)?.input ?? "");
      }
      rows.push(line);
    }
    this.copied = { rows, cutFrom: cut ? { sheet, range } : null };
  }

  async paste(sheet: number, col: number, row: number): Promise<Range | null> {
    if (!this.copied) return null;
    const { rows, cutFrom } = this.copied;
    if (cutFrom) {
      await this.clearCells(cutFrom.sheet, cutFrom.range);
      this.copied = null;
    }
    for (const [dy, line] of rows.entries()) {
      for (const [dx, input] of line.entries()) await this.setCell(sheet, col + dx, row + dy, input);
    }
    return { col, row, cols: rows[0].length, rows: rows.length };
  }

  private inRange(sheet: number, range: Range): [string, CellView][] {
    return [...this.cells].filter(
      ([key, cell]) =>
        key.startsWith(`${sheet}:`) &&
        cell.col >= range.col &&
        cell.col < range.col + range.cols &&
        cell.row >= range.row &&
        cell.row < range.row + range.rows,
    );
  }

  async setFrozen(): Promise<void> {}

  async setHidden(): Promise<void> {}

  async find(sheet: number, query: string): Promise<CellPos[]> {
    if (query === "") return [];
    const wanted = query.toLowerCase();
    return [...this.cells]
      .filter(([key, cell]) => key.startsWith(`${sheet}:`) && cell.input.toLowerCase().includes(wanted))
      .map(([, cell]) => ({ col: cell.col, row: cell.row }))
      .sort((a, b) => a.row - b.row || a.col - b.col);
  }

  async replace(sheet: number, col: number, row: number, query: string, replacement: string): Promise<boolean> {
    const input = this.cells.get(`${sheet}:${col},${row}`)?.input ?? "";
    if (query === "" || !input.toLowerCase().includes(query.toLowerCase())) return false;
    const escaped = query.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    await this.setCell(sheet, col, row, input.replace(new RegExp(escaped, "gi"), () => replacement));
    return true;
  }

  async replaceAll(sheet: number, query: string, replacement: string): Promise<number> {
    let changed = 0;
    for (const cell of await this.find(sheet, query)) {
      if (await this.replace(sheet, cell.col, cell.row, query, replacement)) changed++;
    }
    return changed;
  }

  async sort(): Promise<Range> {
    throw new Error("a pré-visualização não classifica");
  }

  async filterOptions(): Promise<FilterOptions> {
    throw new Error("a pré-visualização não filtra");
  }

  async applyFilter(): Promise<number> {
    return 0;
  }

  async pivots(): Promise<Pivot[]> {
    return [];
  }

  async pivotFields(): Promise<string[]> {
    return [];
  }

  async pivotItems(): Promise<string[]> {
    return [];
  }

  async addPivot(): Promise<Pivot> {
    throw new Error("a pré-visualização não tem tabelas dinâmicas");
  }

  async updatePivot(): Promise<Pivot> {
    throw new Error("a pré-visualização não tem tabelas dinâmicas");
  }

  async refreshPivot(): Promise<Pivot> {
    throw new Error("a pré-visualização não tem tabelas dinâmicas");
  }

  async deletePivot(): Promise<void> {}

  async refreshAllPivots(): Promise<number> {
    return 0;
  }

  async validations(): Promise<Validation[]> {
    return [];
  }

  async addValidation(): Promise<Validation> {
    throw new Error("a pré-visualização não tem validação de dados");
  }

  async updateValidation(): Promise<Validation> {
    throw new Error("a pré-visualização não tem validação de dados");
  }

  async deleteValidation(): Promise<void> {}

  async listOptions(): Promise<string[] | null> {
    return null;
  }

  async alert(text: string): Promise<void> {
    window.alert(text);
  }

  async condRules(): Promise<CondRule[]> {
    return [];
  }

  async addCondRule(): Promise<void> {
    throw new Error("a pré-visualização não tem formatação condicional");
  }

  async updateCondRule(): Promise<void> {}

  async deleteCondRule(): Promise<void> {}

  async dataRegion(_sheet: number, col: number, row: number): Promise<Range> {
    return { col, row, cols: 1, rows: 1 };
  }

  async charts(): Promise<ChartInfo[]> {
    return [];
  }

  async addChart(): Promise<ChartInfo> {
    throw new Error("a pré-visualização não tem gráficos");
  }

  async addPivotChart(): Promise<ChartInfo> {
    throw new Error("a pré-visualização não tem gráficos");
  }

  async updateChart(chart: ChartInfo): Promise<ChartInfo> {
    return chart;
  }

  async deleteChart(): Promise<void> {}

  async undo(): Promise<void> {}

  async redo(): Promise<void> {}
}

export function createEngine(): Engine {
  return isTauri() ? new TauriEngine() : new PreviewEngine();
}
