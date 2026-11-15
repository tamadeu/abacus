import {
  createEngine,
  type BorderKind,
  type CellView,
  type ChartInfo,
  type ChartKind,
  type Direction,
  type FilterOptions,
  type Validation,
  type Range,
  type SheetInfo,
  type Structure,
  type StyleProperty,
} from "./engine";
import { CHART_HANDLE, drawChart, type Box } from "./chart-draw";
import { setupConditionalPanel } from "./conditional";
import { setupPivotPanel } from "./pivot";
import { setupValidationPanel } from "./validation";
import FUNCTIONS from "./functions.json";
import { formulaRefs } from "./refs";

const HEADER_WIDTH = 48;
const HEADER_HEIGHT = 26;
// Limites de uma planilha, iguais aos do Excel.
const MAX_COLS = 16_384;
const MAX_ROWS = 1_048_576;
const FONT = "Inter, system-ui, sans-serif";
/** Espaço entre o texto e a borda da célula. */
const PADDING = 8;
/** Largura padrão de uma coluna, igual à do motor. Só colunas com ela se alargam sozinhas. */
const DEFAULT_COL_WIDTH = 90;
/** Distância, em pixels, em que o mouse "pega" a borda de um cabeçalho. */
const GRIP = 4;
/** Menor largura de coluna ou altura de linha ao arrastar. */
const MIN_SIZE = 8;

const COLORS = {
  ink: "#1b1b29",
  muted: "#5b5b6b",
  line: "#e4e4ec",
  header: "#f6f6fa",
  headerActive: "#dcd9fa",
  headerActiveInk: "#3730a3",
  accent: "#4f46e5",
  selection: "rgba(79, 70, 229, 0.1)",
  /** Divisa entre a parte congelada e a que rola. */
  frozenLine: "#9d9db5",
  /** Fundo das células encontradas pelo "Localizar". */
  found: "rgba(250, 204, 21, 0.4)",
  error: "#b42318",
};

/** Cores dos intervalos citados na fórmula em edição, na ordem em que aparecem. */
const REF_COLORS = ["#2563eb", "#c2410c", "#7c3aed", "#0f766e", "#be185d", "#4d7c0f"];
/** Altura de cada linha de texto dentro de uma célula. */
const LINE_HEIGHT = 18;
/** Meio lado, em pixels, da área em que o mouse "pega" a alça de preenchimento. */
const HANDLE_GRIP = 6;

const engine = createEngine();
const $ = <T extends HTMLElement>(selector: string) => document.querySelector<T>(selector)!;
const wrap = $("#grid-wrap");
const canvas = $<HTMLCanvasElement>("#grid");
const ctx = canvas.getContext("2d")!;
const nameBox = $("#name-box");
const fx = $<HTMLInputElement>("#fx");
const editor = $<HTMLInputElement>("#cell-editor");
const menu = $("#menu");
const mergeButton = $("#merge-btn");
const borderSelect = $<HTMLSelectElement>("#borders");
const valignSelect = $<HTMLSelectElement>("#valign");
const suggest = $("#suggest");
const findBar = $("#find");
const findQuery = $<HTMLInputElement>("#find-query");
const findCount = $("#find-count");
const replaceText = $<HTMLInputElement>("#replace-text");
const filterPanel = $("#filter");
const filterTitle = $("#filter-title");
const filterValues = $("#filter-values");
const chartPanel = $("#chart-panel");
const chartTitle = $<HTMLInputElement>("#chart-title");
const chartKindSelect = $<HTMLSelectElement>("#chart-kind");
const chartRange = $<HTMLInputElement>("#chart-range");
const chartError = $("#chart-error");

/** Estilos que a barra de ferramentas liga e desliga. */
type ToggleProperty = "bold" | "italic" | "underline" | "strike" | "wrap";
const statusEl = $("#status");
const statsEl = $("#stats");
const docName = $("#doc-name");
const docState = $("#doc-state");
const sheetTabs = $("#sheet-tabs");
const textColor = $<HTMLInputElement>("#text-color");
const fillColor = $<HTMLInputElement>("#fill-color");

interface Cell {
  col: number;
  row: number;
}

/** Onde a pessoa está em uma planilha. */
interface Position {
  /** Célula ativa: a que recebe o que for digitado. */
  anchor: Cell;
  /** Canto oposto da seleção. Igual à âncora quando há uma célula só. */
  focus: Cell;
  /** Primeira coluna e linha visíveis (a rolagem é por célula inteira). */
  origin: Cell;
}

const newPosition = (): Position => ({
  anchor: { col: 0, row: 0 },
  focus: { col: 0, row: 0 },
  origin: { col: 0, row: 0 },
});

/** Planilhas da pasta aberta e a posição guardada em cada uma. */
let sheets: SheetInfo[] = [];
let positions = new Map<number, Position>();
/** Índice da planilha na tela. */
let sheet = 0;
let pos = newPosition();
/** Células da tela, por "coluna,linha". */
let cells = new Map<string, CellView>();
/** Áreas mescladas da tela. */
let merges: Range[] = [];
/** Células que ficam escondidas dentro de uma área mesclada, por "coluna,linha". */
let covered = new Set<string>();
/**
 * Tamanhos das colunas e linhas da tela: primeiro as congeladas, depois as
 * que rolam, a partir de `pos.origin`. As ocultas têm tamanho zero.
 */
let colWidths: number[] = [];
let rowHeights: number[] = [];
/** Quantas colunas e linhas da planilha estão congeladas na tela. */
let frozen = { cols: 0, rows: 0 };
/** Posição em pixels de cada borda: `colX[i]` é onde começa a i-ésima coluna da tela. */
let colX: number[] = [HEADER_WIDTH];
let rowY: number[] = [HEADER_HEIGHT];
/** Há texto na barra de fórmulas que ainda não foi gravado na célula. */
let dirty = false;
/** O botão do mouse está pressionado, estendendo a seleção. */
let dragging = false;

/** Borda de cabeçalho que redimensiona a coluna ou linha de índice `index` na tela. */
interface Grip {
  kind: "col" | "row";
  index: number;
}

/** Coluna ou linha sendo redimensionada com o mouse. */
let resizing: Grip | null = null;
/** O mouse já se moveu desde que a borda foi pega. */
let resized = false;
/**
 * Edição direto na célula: `null` quando o editor está fechado. No modo
 * "enter" (começou digitando) as setas gravam e andam; no modo "edit" (F2 ou
 * dois cliques) elas movem o cursor dentro do texto.
 */
let editMode: "enter" | "edit" | null = null;
/**
 * Referência inserida na fórmula clicando numa célula: o trecho do texto que
 * ela ocupa e a célula onde o clique começou. Arrastar troca esse trecho.
 */
let pointing: { input: HTMLInputElement; start: number; end: number; from: Cell } | null = null;
/** Cabeçalhos sendo arrastados para selecionar colunas ou linhas inteiras. */
let headerDrag: { kind: "col" | "row"; from: Cell } | null = null;
/** Última posição do mouse durante um arrasto, para a rolagem automática. */
let lastPointer = { px: 0, py: 0 };
/** Temporizador da rolagem automática, ativo enquanto o mouse está além da borda. */
let scrollTimer: number | null = null;
/** Uma rolagem automática está esperando a resposta do motor. */
let scrolling = false;
/** Funções sugeridas para o nome que está sendo digitado, e onde ele fica no texto. */
let suggestion: { input: HTMLInputElement; start: number; end: number; names: string[]; index: number } | null = null;
/** O mouse está arrastando para apontar um intervalo na fórmula. */
let pointDrag = false;
/** Destino do preenchimento enquanto a alça é arrastada; `null` se ainda não saiu da seleção. */
let fillTarget: { axis: "rows" | "cols"; to: number } | null = null;
/** A alça de preenchimento está sendo arrastada. */
let fillDrag = false;
/** Intervalo copiado ou recortado, que fica marcado com borda tracejada. */
let copiedMark: { sheet: number; range: Range; cut: boolean } | null = null;
/** Células encontradas pelo "Localizar", em ordem de leitura, e as mesmas por "coluna,linha". */
let found: Cell[] = [];
let foundKeys = new Set<string>();
/** Filtro que o painel está montando: a tabela, a coluna e os valores dela. */
let filtering: FilterOptions | null = null;
/** Regras de validação da planilha aberta, para a seta da lista suspensa. */
let validations: Validation[] = [];
/** Arquivo aberto, ou `null` para uma pasta nova. */
let filePath: string | null = null;
/** A pasta tem alterações que não estão no arquivo. */
let modified = false;
/** Gráficos da planilha na tela, com os dados que mostram. */
let charts: ChartInfo[] = [];
/** Gráfico selecionado (o `id`), se houver. */
let selectedChart: number | null = null;
/** Gráfico sendo movido ou redimensionado com o mouse. */
let chartDrag: { id: number; mode: "move" | "resize"; grab: Cell; origin: ChartInfo; moved: boolean } | null = null;

const key = (col: number, row: number) => `${col},${row}`;
const clamp = (value: number, max: number) => Math.min(Math.max(value, 0), max);

function colName(col: number): string {
  let name = "";
  for (let n = col + 1; n > 0; n = Math.floor((n - 1) / 26)) {
    name = String.fromCharCode(65 + ((n - 1) % 26)) + name;
  }
  return name;
}

const cellName = (cell: Cell) => colName(cell.col) + (cell.row + 1);

const contains = (range: Range, col: number, row: number) =>
  col >= range.col && col < range.col + range.cols && row >= range.row && row < range.row + range.rows;

/** Área mesclada que contém a célula, se houver. */
const mergeAt = (cell: Cell) => merges.find((merge) => contains(merge, cell.col, cell.row));

/** A célula como intervalo: a área mesclada a que pertence, ou só ela. */
const blockOf = (cell: Cell): Range => mergeAt(cell) ?? { col: cell.col, row: cell.row, cols: 1, rows: 1 };

/**
 * Retângulo selecionado, da âncora ao foco, aumentado até conter por inteiro
 * toda área mesclada que tocar.
 */
function selectedRange(): Range {
  const { anchor, focus } = pos;
  let left = Math.min(anchor.col, focus.col);
  let top = Math.min(anchor.row, focus.row);
  let right = Math.max(anchor.col, focus.col);
  let bottom = Math.max(anchor.row, focus.row);
  for (let grew = true; grew; ) {
    grew = false;
    for (const merge of merges) {
      const mergeRight = merge.col + merge.cols - 1;
      const mergeBottom = merge.row + merge.rows - 1;
      const touches = merge.col <= right && mergeRight >= left && merge.row <= bottom && mergeBottom >= top;
      const inside = merge.col >= left && mergeRight <= right && merge.row >= top && mergeBottom <= bottom;
      if (!touches || inside) continue;
      left = Math.min(left, merge.col);
      top = Math.min(top, merge.row);
      right = Math.max(right, mergeRight);
      bottom = Math.max(bottom, mergeBottom);
      grew = true;
    }
  }
  return { col: left, row: top, cols: right - left + 1, rows: bottom - top + 1 };
}

/** A seleção é uma célula só (ou uma única área mesclada). */
function singleSelection(): boolean {
  const range = selectedRange();
  const block = blockOf(pos.anchor);
  return range.cols === block.cols && range.rows === block.rows;
}

/**
 * Célula a um passo da ativa, pulando por cima da área mesclada em que ela
 * está e das linhas e colunas ocultas que estiverem na tela.
 */
function step(dx: number, dy: number): Cell {
  const block = blockOf(pos.anchor);
  let col = dx > 0 ? block.col + block.cols : dx < 0 ? block.col - 1 : pos.anchor.col;
  let row = dy > 0 ? block.row + block.rows : dy < 0 ? block.row - 1 : pos.anchor.row;
  while (dx !== 0 && col > 0 && col < MAX_COLS - 1 && colOnScreen(col) && colWidths[colBand(col)] === 0) col += dx;
  while (dy !== 0 && row > 0 && row < MAX_ROWS - 1 && rowOnScreen(row) && rowHeights[rowBand(row)] === 0) row += dy;
  return { col, row };
}

/** Quebra um texto em linhas que caibam na largura, sem cortar palavras. */
function wrapLines(text: string, maxWidth: number): string[] {
  const lines: string[] = [];
  for (const paragraph of text.split("\n")) {
    let line = "";
    for (const word of paragraph.split(" ")) {
      const candidate = line === "" ? word : `${line} ${word}`;
      if (line !== "" && ctx.measureText(candidate).width > maxWidth) {
        lines.push(line);
        line = word;
      } else {
        line = candidate;
      }
    }
    lines.push(line);
  }
  return lines;
}

/** Bordas acumuladas: começa em `start` e soma cada tamanho. */
function edges(sizes: number[], start: number): number[] {
  const out = [start];
  for (const size of sizes) out.push(out[out.length - 1] + size);
  return out;
}

/**
 * Faixa da tela (índice em `colWidths`) de uma coluna. As congeladas vêm
 * primeiro; para as demais o índice pode cair fora da tela.
 */
const colBand = (col: number) => (col < frozen.cols ? col : col - pos.origin.col + frozen.cols);
const rowBand = (row: number) => (row < frozen.rows ? row : row - pos.origin.row + frozen.rows);
/** Coluna ou linha que ocupa uma faixa da tela. */
const bandCol = (band: number) => (band < frozen.cols ? band : pos.origin.col + band - frozen.cols);
const bandRow = (band: number) => (band < frozen.rows ? band : pos.origin.row + band - frozen.rows);
/** A coluna está na tela: é congelada ou está na parte que rola, sem ter ficado para trás. */
const colOnScreen = (col: number) => col < frozen.cols || (col >= pos.origin.col && colBand(col) < colWidths.length);
const rowOnScreen = (row: number) => row < frozen.rows || (row >= pos.origin.row && rowBand(row) < rowHeights.length);

/**
 * Pixel onde começa a coluna, limitado à área visível: o que rolou para
 * trás fica na divisa da parte congelada, e o que está adiante, no fim.
 */
const xOf = (col: number) => colX[Math.min(Math.max(colBand(col), Math.min(col, frozen.cols)), colX.length - 1)];
/** Pixel onde começa a linha, limitado à área visível. */
const yOf = (row: number) => rowY[Math.min(Math.max(rowBand(row), Math.min(row, frozen.rows)), rowY.length - 1)];

/** Índice da faixa que contém o pixel, dadas as bordas acumuladas. */
function bandAt(bounds: number[], pixel: number): number {
  let index = 0;
  while (index < bounds.length - 2 && bounds[index + 1] <= pixel) index++;
  return index;
}

/** Borda de cabeçalho sob um ponto do canvas, se houver. */
function gripAt(px: number, py: number): Grip | null {
  const near = (bounds: number[], pixel: number) => {
    for (let index = 1; index < bounds.length; index++) {
      if (Math.abs(bounds[index] - pixel) <= GRIP) return index - 1;
    }
    return -1;
  };
  if (py < HEADER_HEIGHT && px > HEADER_WIDTH) {
    const index = near(colX, px);
    return index < 0 ? null : { kind: "col", index };
  }
  if (px < HEADER_WIDTH && py > HEADER_HEIGHT) {
    const index = near(rowY, py);
    return index < 0 ? null : { kind: "row", index };
  }
  return null;
}

/** Tamanho da fonte de uma célula em relação ao padrão: 1 é o padrão, 2 é o dobro. */
const fontScale = (cell: CellView) => (cell.style?.size ?? 100) / 100;

/** Fonte usada para desenhar o texto de uma célula. */
const cellFont = (cell: CellView) =>
  `${cell.style?.italic ? "italic " : ""}${cell.style?.bold ? "700 " : ""}${Math.round(140 * fontScale(cell)) / 10}px ${FONT}`;

/** Retângulo de um intervalo na tela, limitado à área visível. */
function rectOf(range: Range) {
  const left = xOf(range.col);
  const top = yOf(range.row);
  return { left, top, width: xOf(range.col + range.cols) - left, height: yOf(range.row + range.rows) - top };
}

/**
 * Índice, em `colX` ou `rowY`, da borda onde começa a coluna ou linha
 * `index`, ou -1 se essa borda não está na tela.
 */
function edgeBand(index: number, frozenCount: number, bandOf: (index: number) => number, edgeCount: number): number {
  if (index <= frozenCount) return index;
  const band = bandOf(index);
  return band > frozenCount && band < edgeCount ? band : -1;
}

/** Centro da alça de preenchimento, ou `null` se o canto da seleção não está na tela. */
function fillHandle(): { x: number; y: number } | null {
  if (editMode !== null) return null;
  const range = selectedRange();
  // Borda depois da última coluna e da última linha da seleção.
  const colIndex = edgeBand(range.col + range.cols, frozen.cols, colBand, colX.length);
  const rowIndex = edgeBand(range.row + range.rows, frozen.rows, rowBand, rowY.length);
  if (colIndex < 0 || rowIndex < 0) return null;
  return { x: colX[colIndex], y: rowY[rowIndex] };
}

/** A seleção somada ao que a alça de preenchimento vai preencher. */
function fillRange(): Range {
  const range = selectedRange();
  if (!fillTarget) return range;
  if (fillTarget.axis === "rows") {
    const first = Math.min(range.row, fillTarget.to);
    const last = Math.max(range.row + range.rows - 1, fillTarget.to);
    return { ...range, row: first, rows: last - first + 1 };
  }
  const first = Math.min(range.col, fillTarget.to);
  const last = Math.max(range.col + range.cols - 1, fillTarget.to);
  return { ...range, col: first, cols: last - first + 1 };
}

/** Fórmula em edição, na célula ou na barra, ou `null` se não há uma. */
function formulaDraft(): string | null {
  const editingNow = editMode !== null || document.activeElement === fx;
  return editingNow && fx.value.startsWith("=") ? fx.value : null;
}

/** Célula sob um ponto do canvas. */
function cellAt(px: number, py: number): Cell {
  return {
    col: clamp(bandCol(bandAt(colX, px)), MAX_COLS - 1),
    row: clamp(bandRow(bandAt(rowY, py)), MAX_ROWS - 1),
  };
}

/** Retângulo do gráfico na tela, ou `null` se o canto dele não está visível. */
function chartBox(chart: ChartInfo): Box | null {
  if (!colOnScreen(chart.fromCol) || !rowOnScreen(chart.fromRow)) return null;
  const left = xOf(chart.fromCol);
  const top = yOf(chart.fromRow);
  const width = xOf(chart.toCol + 1) - left;
  const height = yOf(chart.toRow + 1) - top;
  return width > 0 && height > 0 ? { left, top, width, height } : null;
}

/** O gráfico mais acima sob um ponto do canvas. */
function chartAt(px: number, py: number): ChartInfo | null {
  for (const chart of [...charts].reverse()) {
    const box = chartBox(chart);
    if (box && px >= box.left && px <= box.left + box.width && py >= box.top && py <= box.top + box.height) return chart;
  }
  return null;
}

/** O ponto está na alça de redimensionar do gráfico? */
function onChartHandle(chart: ChartInfo, px: number, py: number): boolean {
  const box = chartBox(chart);
  return box !== null && px >= box.left + box.width - CHART_HANDLE - 3 && py >= box.top + box.height - CHART_HANDLE - 3;
}

/** A seta da lista suspensa, se a célula ativa tem uma regra de lista e está na tela. */
function listArrow(): { left: number; top: number; size: number } | null {
  if (editMode !== null || !singleSelection()) return null;
  const { anchor } = pos;
  const hit = validations.some((rule) => {
    if (rule.kind !== "list") return false;
    const area = parseRange(rule.range);
    return (
      area !== null &&
      anchor.col >= area.col &&
      anchor.col < area.col + area.cols &&
      anchor.row >= area.row &&
      anchor.row < area.row + area.rows
    );
  });
  if (!hit || !colOnScreen(anchor.col) || !rowOnScreen(anchor.row)) return null;
  const box = rectOf(blockOf(anchor));
  const size = Math.min(18, box.height - 2);
  if (size < 8 || box.width < size + 8) return null;
  return { left: box.left + box.width - size - 1, top: box.top + box.height - size - 1, size };
}

const dvMenu = $("#dv-menu");
const closeListMenu = () => {
  dvMenu.hidden = true;
};

/** Abre a lista suspensa da célula ativa, logo abaixo dela. */
async function openListMenu() {
  const { anchor } = pos;
  let items: string[] | null;
  try {
    items = await engine.listOptions(sheet, anchor.col, anchor.row);
  } catch (error) {
    statusEl.textContent = `Erro ao ler a lista: ${error}`;
    return;
  }
  if (!items || items.length === 0) {
    statusEl.textContent = "A lista não tem itens.";
    return;
  }
  const current = cells.get(key(anchor.col, anchor.row))?.display ?? "";
  dvMenu.replaceChildren(
    ...items.map((item) => {
      const option = document.createElement("li");
      option.setAttribute("role", "option");
      option.setAttribute("aria-selected", String(item === current));
      option.textContent = item;
      option.addEventListener("pointerdown", (event) => {
        event.preventDefault();
        closeListMenu();
        void pickListItem(item);
      });
      return option;
    }),
  );
  const bounds = canvas.getBoundingClientRect();
  const box = rectOf(blockOf(anchor));
  dvMenu.hidden = false;
  dvMenu.style.minWidth = `${Math.max(140, box.width)}px`;
  dvMenu.style.left = `${Math.max(0, Math.min(bounds.left + box.left, window.innerWidth - dvMenu.offsetWidth - 8))}px`;
  dvMenu.style.top = `${Math.min(bounds.top + box.top + box.height + 2, window.innerHeight - dvMenu.offsetHeight - 8)}px`;
}

/** Grava na célula ativa o item escolhido na lista suspensa. */
async function pickListItem(item: string) {
  fx.value = item;
  dirty = true;
  if (await write()) await refresh();
  canvas.focus();
}

function draw() {
  const width = wrap.clientWidth;
  const height = wrap.clientHeight;
  const dpr = window.devicePixelRatio || 1;
  canvas.width = Math.round(width * dpr);
  canvas.height = Math.round(height * dpr);
  canvas.style.width = `${width}px`;
  canvas.style.height = `${height}px`;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);

  const { origin, anchor } = pos;
  const range = selectedRange();
  const gridLines = (toX: number, toY: number) => {
    ctx.strokeStyle = COLORS.line;
    ctx.lineWidth = 1;
    ctx.beginPath();
    for (const x of colX) {
      ctx.moveTo(x + 0.5, 0);
      ctx.lineTo(x + 0.5, toY);
    }
    for (const y of rowY) {
      ctx.moveTo(0, y + 0.5);
      ctx.lineTo(toX, y + 0.5);
    }
    ctx.stroke();
  };

  ctx.fillStyle = "#fff";
  ctx.fillRect(0, 0, width, height);
  gridLines(width, height);

  // Áreas mescladas: apaga as linhas de grade de dentro.
  ctx.fillStyle = "#fff";
  for (const merge of merges) {
    const rect = rectOf(merge);
    ctx.fillRect(rect.left + 1, rect.top + 1, rect.width - 1, rect.height - 1);
  }

  // Fundos coloridos, antes do texto. Como no Excel, cobrem as linhas de
  // grade à direita e abaixo da célula.
  for (const cell of cells.values()) {
    if (!cell.style?.fill || covered.has(key(cell.col, cell.row))) continue;
    const rect = rectOf(blockOf(cell));
    ctx.fillStyle = cell.style.fill;
    ctx.fillRect(rect.left + 1, rect.top + 1, rect.width, rect.height);
  }

  // Barras de dados: vão do zero até o valor, por cima do fundo e sob o texto.
  for (const cell of cells.values()) {
    if (!cell.bar || covered.has(key(cell.col, cell.row))) continue;
    const rect = rectOf(blockOf(cell));
    if (rect.width <= 4 || rect.height <= 4) continue;
    const { value, axis, color } = cell.bar;
    const from = rect.left + 1 + Math.min(value, axis) * (rect.width - 2);
    const length = Math.max(1, Math.abs(value - axis) * (rect.width - 2));
    const gradient = ctx.createLinearGradient(from, 0, from + length, 0);
    gradient.addColorStop(0, color);
    gradient.addColorStop(1, `${color}55`);
    ctx.fillStyle = gradient;
    ctx.fillRect(from, rect.top + 3, length, rect.height - 5);
  }

  // Células encontradas pelo "Localizar"
  ctx.fillStyle = COLORS.found;
  for (const cell of found) {
    if (!colOnScreen(cell.col) || !rowOnScreen(cell.row)) continue;
    const rect = rectOf(blockOf(cell));
    ctx.fillRect(rect.left + 1, rect.top + 1, rect.width - 1, rect.height - 1);
  }

  // Bordas, cada uma no lado da sua célula: cima, direita, baixo, esquerda
  for (const cell of cells.values()) {
    const borders = cell.style?.borders;
    if (!borders?.some(Boolean)) continue;
    const rect = rectOf({ col: cell.col, row: cell.row, cols: 1, rows: 1 });
    if (rect.width <= 0 || rect.height <= 0) continue;
    const x1 = rect.left + 0.5;
    const y1 = rect.top + 0.5;
    const x2 = x1 + rect.width;
    const y2 = y1 + rect.height;
    const sides = [
      [x1, y1, x2, y1],
      [x2, y1, x2, y2],
      [x1, y2, x2, y2],
      [x1, y1, x1, y2],
    ];
    borders.forEach((border, side) => {
      if (!border) return;
      ctx.strokeStyle = border.color;
      ctx.lineWidth = border.width;
      ctx.setLineDash(border.dashed ? [3, 2] : []);
      ctx.beginPath();
      ctx.moveTo(sides[side][0], sides[side][1]);
      ctx.lineTo(sides[side][2], sides[side][3]);
      ctx.stroke();
    });
  }
  ctx.setLineDash([]);

  // Conteúdo das células
  ctx.textBaseline = "middle";
  for (const cell of cells.values()) {
    if (cell.display === "" || covered.has(key(cell.col, cell.row))) continue;
    const style = cell.style;
    const block = blockOf(cell);
    const { left, top, width: cellWidth, height: cellHeight } = rectOf(block);
    if (cellWidth <= 0 || cellHeight <= 0) continue;
    const align = style?.align ?? (cell.kind === "number" ? "right" : cell.kind === "error" ? "center" : "left");
    const anchorX = align === "right" ? left + cellWidth - PADDING : align === "center" ? left + cellWidth / 2 : left + PADDING;

    ctx.save();
    ctx.font = cellFont(cell);
    // Como no Excel, um número que não cabe na coluna vira uma fileira de "#".
    let text = cell.display;
    if (cell.kind === "number" && ctx.measureText(text).width > cellWidth - 2 * PADDING) {
      text = "#".repeat(Math.max(0, Math.floor((cellWidth - 2 * PADDING) / ctx.measureText("#").width)));
    }
    const wraps = style?.wrap === true && cell.kind === "text";
    const lines = wraps ? wrapLines(text, cellWidth - 2 * PADDING) : [text];
    // Como no Excel, texto alinhado à esquerda invade as células vazias à
    // direita, a menos que quebre em linhas ou esteja numa área mesclada.
    let clipWidth = cellWidth;
    if (cell.kind === "text" && align === "left" && !wraps && block.cols === 1) {
      const needed = ctx.measureText(text).width + 2 * PADDING;
      const lastCol = bandCol(colWidths.length - 1);
      for (let next = cell.col + 1; needed > clipWidth && next <= lastCol; next++) {
        // Da parte congelada não passa para a que rola se há colunas escondidas entre elas.
        if (next === frozen.cols && origin.col !== frozen.cols) break;
        if (cells.get(key(next, cell.row))?.display || mergeAt({ col: next, row: cell.row })) break;
        clipWidth = xOf(next + 1) - left;
      }
    }
    ctx.beginPath();
    ctx.rect(left + 1, top + 1, clipWidth - 1, cellHeight - 1);
    ctx.clip();
    ctx.fillStyle = style?.color ?? (cell.kind === "error" ? COLORS.error : COLORS.ink);
    ctx.textAlign = align;
    // O bloco de linhas fica embaixo, como no Excel, a menos que o estilo
    // peça outro alinhamento vertical. Se não couber, começa do topo.
    const lineHeight = Math.round(LINE_HEIGHT * fontScale(cell));
    const spare = cellHeight - lines.length * lineHeight;
    const offset = style?.valign === "top" ? 3 : style?.valign === "center" ? spare / 2 : spare - 3;
    const blockTop = top + Math.max(3, offset);
    lines.forEach((line, index) => {
      const middle = blockTop + index * lineHeight + lineHeight / 2 + 1;
      ctx.fillText(line, anchorX, middle);
      if (style?.underline || style?.strike) {
        const lineWidth = ctx.measureText(line).width;
        const start = align === "right" ? anchorX - lineWidth : align === "center" ? anchorX - lineWidth / 2 : anchorX;
        if (style.underline) ctx.fillRect(start, middle + lineHeight * 0.44, lineWidth, 1);
        if (style.strike) ctx.fillRect(start, middle, lineWidth, 1);
      }
    });
    ctx.restore();
  }

  // Gráficos, sobre as células
  for (const chart of charts) {
    const box = chartBox(chart);
    if (box) drawChart(ctx, chart, box, chart.id === selectedChart);
  }

  // Seleção: o intervalo fica tingido, menos a célula ativa.
  const selLeft = xOf(range.col);
  const selTop = yOf(range.row);
  const selWidth = xOf(range.col + range.cols) - selLeft;
  const selHeight = yOf(range.row + range.rows) - selTop;
  if (selWidth > 0 && selHeight > 0) {
    if (!singleSelection()) {
      const active = rectOf(blockOf(anchor));
      ctx.fillStyle = COLORS.selection;
      ctx.beginPath();
      ctx.rect(selLeft, selTop, selWidth, selHeight);
      ctx.rect(active.left, active.top, active.width, active.height);
      ctx.fill("evenodd");
    }
    ctx.strokeStyle = COLORS.accent;
    ctx.lineWidth = 2;
    ctx.strokeRect(selLeft + 0.5, selTop + 0.5, selWidth, selHeight);
  }

  // Seta da lista suspensa na célula ativa
  const arrow = listArrow();
  if (arrow) {
    ctx.fillStyle = "#fff";
    ctx.fillRect(arrow.left, arrow.top, arrow.size, arrow.size);
    ctx.strokeStyle = COLORS.accent;
    ctx.lineWidth = 1;
    ctx.strokeRect(arrow.left + 0.5, arrow.top + 0.5, arrow.size - 1, arrow.size - 1);
    ctx.fillStyle = COLORS.accent;
    ctx.beginPath();
    const mid = arrow.left + arrow.size / 2;
    const center = arrow.top + arrow.size / 2;
    ctx.moveTo(mid - 4, center - 2);
    ctx.lineTo(mid + 4, center - 2);
    ctx.lineTo(mid, center + 3);
    ctx.fill();
  }

  // Intervalos citados na fórmula em edição, cada um com uma cor
  const formula = formulaDraft();
  if (formula !== null) {
    formulaRefs(formula).forEach((ref, index) => {
      const rect = rectOf(ref);
      if (rect.width <= 0 || rect.height <= 0) return;
      const color = REF_COLORS[index % REF_COLORS.length];
      ctx.globalAlpha = 0.1;
      ctx.fillStyle = color;
      ctx.fillRect(rect.left + 1, rect.top + 1, rect.width - 1, rect.height - 1);
      ctx.globalAlpha = 1;
      ctx.strokeStyle = color;
      ctx.lineWidth = 2;
      ctx.strokeRect(rect.left + 0.5, rect.top + 0.5, rect.width, rect.height);
    });
  }

  // Alça de preenchimento no canto da seleção, e o que ela vai preencher
  const handle = fillHandle();
  if (handle) {
    ctx.fillStyle = "#fff";
    ctx.fillRect(handle.x - 4, handle.y - 4, 9, 9);
    ctx.fillStyle = COLORS.accent;
    ctx.fillRect(handle.x - 3, handle.y - 3, 7, 7);
  }
  if (fillTarget) {
    const rect = rectOf(fillRange());
    ctx.strokeStyle = COLORS.accent;
    ctx.lineWidth = 1;
    ctx.setLineDash([3, 3]);
    ctx.strokeRect(rect.left + 0.5, rect.top + 0.5, rect.width, rect.height);
    ctx.setLineDash([]);
  }

  // Borda tracejada no intervalo copiado ou recortado
  if (copiedMark?.sheet === sheet) {
    const mark = copiedMark.range;
    const markWidth = xOf(mark.col + mark.cols) - xOf(mark.col);
    const markHeight = yOf(mark.row + mark.rows) - yOf(mark.row);
    if (markWidth > 0 && markHeight > 0) {
      ctx.strokeStyle = COLORS.accent;
      ctx.lineWidth = 2;
      ctx.setLineDash([5, 4]);
      ctx.strokeRect(xOf(mark.col) + 0.5, yOf(mark.row) + 0.5, markWidth, markHeight);
      ctx.setLineDash([]);
    }
  }

  // Cabeçalhos, por cima das células
  ctx.fillStyle = COLORS.header;
  ctx.fillRect(0, 0, width, HEADER_HEIGHT);
  ctx.fillRect(0, 0, HEADER_WIDTH, height);
  ctx.fillStyle = COLORS.headerActive;
  ctx.fillRect(selLeft, 0, selWidth, HEADER_HEIGHT);
  ctx.fillRect(0, selTop, HEADER_WIDTH, selHeight);
  ctx.fillStyle = COLORS.header;
  ctx.fillRect(0, 0, HEADER_WIDTH, HEADER_HEIGHT);
  gridLines(HEADER_WIDTH, HEADER_HEIGHT);

  const inRange = (value: number, start: number, count: number) => value >= start && value < start + count;
  ctx.font = `13px ${FONT}`;
  ctx.textAlign = "center";
  colWidths.forEach((colWidth, index) => {
    if (colWidth < 12) return;
    const col = bandCol(index);
    ctx.fillStyle = inRange(col, range.col, range.cols) ? COLORS.headerActiveInk : COLORS.muted;
    ctx.fillText(colName(col), colX[index] + colWidth / 2, HEADER_HEIGHT / 2);
  });
  rowHeights.forEach((rowHeight, index) => {
    if (rowHeight < 10) return;
    const row = bandRow(index);
    ctx.fillStyle = inRange(row, range.row, range.rows) ? COLORS.headerActiveInk : COLORS.muted;
    ctx.fillText(String(row + 1), HEADER_WIDTH / 2, rowY[index] + rowHeight / 2);
  });

  // Marca no cabeçalho onde há colunas ou linhas ocultas.
  ctx.fillStyle = COLORS.accent;
  colWidths.forEach((colWidth, index) => {
    if (colWidth === 0 && colWidths[index - 1] !== 0) ctx.fillRect(colX[index] - 1, 4, 3, HEADER_HEIGHT - 8);
  });
  rowHeights.forEach((rowHeight, index) => {
    if (rowHeight === 0 && rowHeights[index - 1] !== 0) ctx.fillRect(6, rowY[index] - 1, HEADER_WIDTH - 12, 3);
  });

  // Divisa da parte congelada
  ctx.strokeStyle = COLORS.frozenLine;
  ctx.lineWidth = 1;
  ctx.beginPath();
  if (frozen.cols > 0) {
    ctx.moveTo(colX[frozen.cols] + 0.5, 0);
    ctx.lineTo(colX[frozen.cols] + 0.5, height);
  }
  if (frozen.rows > 0) {
    ctx.moveTo(0, rowY[frozen.rows] + 0.5);
    ctx.lineTo(width, rowY[frozen.rows] + 0.5);
  }
  ctx.stroke();

  positionEditor();
}

/** Põe o editor sobre a célula ativa, largo o bastante para o texto. */
function positionEditor() {
  if (editMode === null) return;
  if (!colOnScreen(pos.anchor.col) || !rowOnScreen(pos.anchor.row)) {
    // A célula saiu da tela com a rolagem: a edição continua, fora de vista.
    editor.style.left = "-9999px";
    return;
  }
  // Numa área mesclada o editor ocupa a área toda.
  const rect = rectOf(blockOf(pos.anchor));
  ctx.font = `14px ${FONT}`;
  const wanted = Math.max(rect.width, ctx.measureText(editor.value).width + 2 * PADDING + 8);
  editor.style.left = `${rect.left}px`;
  editor.style.top = `${rect.top}px`;
  editor.style.width = `${Math.min(wanted, wrap.clientWidth - rect.left) + 1}px`;
  editor.style.height = `${rect.height + 1}px`;
}

/**
 * Abre o editor sobre a célula ativa. `initial` é o texto que substitui o
 * conteúdo (quando a pessoa começou a digitar); `null` edita o que já existe.
 */
function startEdit(initial: string | null, mode: "enter" | "edit") {
  const current = cells.get(key(pos.anchor.col, pos.anchor.row))?.input ?? "";
  editor.value = initial ?? current;
  fx.value = editor.value;
  dirty = initial !== null;
  editMode = mode;
  pointing = null;
  editor.hidden = false;
  positionEditor();
  editor.focus();
  editor.setSelectionRange(editor.value.length, editor.value.length);
  draw();
}

/** Fecha o editor da célula, sem gravar nem descartar o texto. */
function stopEdit() {
  editMode = null;
  pointing = null;
  editor.hidden = true;
  closeSuggestions();
}

/** Sem acentos e em maiúsculas, para "media" encontrar "MÉDIA". */
const plain = (text: string) => text.normalize("NFD").replace(/\p{M}/gu, "").toUpperCase();
const FUNCTION_KEYS = FUNCTIONS.map((name) => [plain(name), name] as const);

function closeSuggestions() {
  suggestion = null;
  suggest.hidden = true;
}

/** Mostra as funções cujo nome começa com o que está sendo digitado na fórmula. */
function updateSuggestions(input: HTMLInputElement) {
  const caret = input.selectionStart ?? input.value.length;
  // O nome em digitação: letras logo depois do "=", de um operador ou de um separador.
  const typed = input.value.startsWith("=")
    ? /(?:^=|[=+\-*/^(;:,<>&\s])(\p{L}[\p{L}\p{N}._]*)$/u.exec(input.value.slice(0, caret))
    : null;
  // No meio de um nome já escrito não há o que sugerir.
  if (!typed || /^[\p{L}\p{N}._(]/u.test(input.value.slice(caret))) {
    closeSuggestions();
    return;
  }
  const prefix = plain(typed[1]);
  const names = FUNCTION_KEYS.filter(([key]) => key.startsWith(prefix))
    .slice(0, 8)
    .map(([, name]) => name);
  if (names.length === 0) {
    closeSuggestions();
    return;
  }
  suggestion = { input, start: caret - typed[1].length, end: caret, names, index: 0 };
  renderSuggestions();
}

function renderSuggestions() {
  if (!suggestion) return;
  const { input, names, index } = suggestion;
  suggest.replaceChildren(
    ...names.map((name, position) => {
      const item = document.createElement("li");
      item.setAttribute("role", "option");
      item.setAttribute("aria-selected", String(position === index));
      item.textContent = name;
      // `mousedown` e não `click`, para o campo não perder o foco.
      item.addEventListener("mousedown", (event) => {
        event.preventDefault();
        acceptSuggestion(position);
      });
      return item;
    }),
  );
  const bounds = input.getBoundingClientRect();
  suggest.hidden = false;
  suggest.style.left = `${Math.max(0, Math.min(bounds.left, window.innerWidth - suggest.offsetWidth - 8))}px`;
  suggest.style.top = `${bounds.bottom + 2}px`;
}

/** Troca o nome em digitação pela função escolhida, já com o parêntese aberto. */
function acceptSuggestion(index: number) {
  if (!suggestion) return;
  const { input, start, end, names } = suggestion;
  const call = `${names[index]}(`;
  setDraft(input.value.slice(0, start) + call + input.value.slice(end));
  input.setSelectionRange(start + call.length, start + call.length);
  closeSuggestions();
  draw();
}

/**
 * Trata as teclas que agem sobre a lista de sugestões e diz se a tecla foi
 * consumida. Como no Excel, Tab aceita a sugestão; Enter grava a célula.
 */
function suggestionKey(event: KeyboardEvent): boolean {
  if (!suggestion) return false;
  if (event.key === "ArrowDown" || event.key === "ArrowUp") {
    const count = suggestion.names.length;
    suggestion.index = (suggestion.index + (event.key === "ArrowDown" ? 1 : count - 1)) % count;
    renderSuggestions();
  } else if (event.key === "Tab") {
    acceptSuggestion(suggestion.index);
  } else if (event.key === "Escape") {
    closeSuggestions();
  } else {
    return false;
  }
  event.preventDefault();
  return true;
}

/** Descarta a edição em andamento. */
function cancelEdit() {
  dirty = false;
  stopEdit();
  syncFormulaBar();
  canvas.focus();
  draw();
}

/** Troca o texto em edição nos dois campos (célula e barra de fórmulas). */
function setDraft(value: string) {
  fx.value = value;
  if (editMode !== null) editor.value = value;
  dirty = true;
}

/** Campo em que a pessoa está editando, se houver. */
function activeInput(): HTMLInputElement | null {
  if (document.activeElement === fx) return fx;
  return editMode !== null ? editor : null;
}

/**
 * Diz se um clique na grade deve inserir uma referência na fórmula em vez de
 * encerrar a edição: vale quando o cursor está logo depois de um operador,
 * parêntese ou separador, ou de uma referência que acabou de ser apontada.
 */
function canPoint(input: HTMLInputElement): boolean {
  if (!input.value.startsWith("=")) return false;
  const caret = input.selectionStart ?? input.value.length;
  if (pointing?.input === input && caret === pointing.end) return true;
  return /[=+\-*/^(;:,<>&]$/.test(input.value.slice(0, caret).trimEnd());
}

/** Insere (ou troca) na fórmula a referência ao intervalo entre duas células. */
function pointAt(input: HTMLInputElement, from: Cell, to: Cell) {
  const first = { col: Math.min(from.col, to.col), row: Math.min(from.row, to.row) };
  const last = { col: Math.max(from.col, to.col), row: Math.max(from.row, to.row) };
  const single = first.col === last.col && first.row === last.row;
  const text = single ? cellName(first) : `${cellName(first)}:${cellName(last)}`;
  const start = pointing?.input === input ? pointing.start : (input.selectionStart ?? input.value.length);
  const end = pointing?.input === input ? pointing.end : (input.selectionEnd ?? start);
  setDraft(input.value.slice(0, start) + text + input.value.slice(end));
  pointing = { input, start, end: start + text.length, from };
  input.setSelectionRange(pointing.end, pointing.end);
  closeSuggestions();
  draw();
}

/** Mostra a seleção na caixa de nome e a célula ativa na barra de fórmulas. */
function syncFormulaBar() {
  const { anchor } = pos;
  const range = selectedRange();
  const last = { col: range.col + range.cols - 1, row: range.row + range.rows - 1 };
  if (range.rows === MAX_ROWS && range.cols === MAX_COLS) nameBox.textContent = "Tudo";
  else if (range.rows === MAX_ROWS) nameBox.textContent = `${colName(range.col)}:${colName(last.col)}`;
  else if (range.cols === MAX_COLS) nameBox.textContent = `${range.row + 1}:${last.row + 1}`;
  else nameBox.textContent = singleSelection() ? cellName(anchor) : `${cellName(range)}:${cellName(last)}`;
  if (!dirty) fx.value = cells.get(key(anchor.col, anchor.row))?.input ?? "";
  scheduleStats();
  pivotPanel.sync(anchor.col, anchor.row);
}

const numberFormat = new Intl.NumberFormat("pt-BR", { maximumFractionDigits: 10 });
let statsTimer: number | undefined;
/** Número do pedido de estatísticas mais recente, para descartar respostas atrasadas. */
let statsRequest = 0;

/**
 * Atualiza o resumo da seleção no rodapé (média, contagem e soma). Espera a
 * seleção sossegar para não pedir contas a cada movimento do mouse.
 */
function scheduleStats() {
  window.clearTimeout(statsTimer);
  const request = ++statsRequest;
  if (singleSelection()) {
    statsEl.textContent = "";
    return;
  }
  statsTimer = window.setTimeout(async () => {
    try {
      const stats = await engine.stats(sheet, selectedRange());
      if (request !== statsRequest) return;
      if (stats.numbers > 0) {
        const average = stats.sum / stats.numbers;
        statsEl.textContent = `Média: ${numberFormat.format(average)}  ·  Contagem: ${stats.count}  ·  Soma: ${numberFormat.format(stats.sum)}`;
      } else {
        statsEl.textContent = stats.count > 1 ? `Contagem: ${stats.count}` : "";
      }
    } catch {
      if (request === statsRequest) statsEl.textContent = "";
    }
  }, 80);
}

/** Faz a barra de ferramentas refletir o estilo da célula ativa. */
function syncToolbar() {
  const style = cells.get(key(pos.anchor.col, pos.anchor.row))?.style;
  for (const button of document.querySelectorAll<HTMLElement>("[data-toggle]")) {
    const property = button.dataset.toggle as ToggleProperty;
    button.setAttribute("aria-pressed", String(style?.[property] ?? false));
  }
  const block = blockOf(pos.anchor);
  mergeButton.setAttribute("aria-pressed", String(block.cols > 1 || block.rows > 1));
  for (const button of document.querySelectorAll<HTMLElement>("[data-align]")) {
    button.setAttribute("aria-pressed", String(style?.align === button.dataset.align));
  }
  textColor.value = style?.color?.toLowerCase() ?? COLORS.ink;
  fillColor.value = style?.fill?.toLowerCase() ?? "#ffffff";
}

/** Atualiza o nome do arquivo e o estado de gravação no cabeçalho. */
function syncDocument() {
  docName.textContent = filePath?.split(/[\\/]/).pop() ?? "Sem título";
  docState.textContent = modified ? "Alterações não salvas" : filePath ? "Salvo" : (engine.notice ?? "");
}

function markModified() {
  modified = true;
  syncDocument();
  // O que foi encontrado pode ter mudado de lugar ou deixado de existir.
  if (!findBar.hidden) void runFind();
}

/** Busca no motor o que cabe na tela e redesenha. */
async function refresh() {
  try {
    const width = Math.max(1, wrap.clientWidth - HEADER_WIDTH);
    const height = Math.max(1, wrap.clientHeight - HEADER_HEIGHT);
    const view = await engine.view(sheet, pos.origin.col, pos.origin.row, width, height);
    colWidths = view.colWidths;
    rowHeights = view.rowHeights;
    frozen = { cols: view.frozenCols, rows: view.frozenRows };
    // A parte que rola nunca começa dentro da congelada.
    pos.origin.col = Math.max(pos.origin.col, frozen.cols);
    pos.origin.row = Math.max(pos.origin.row, frozen.rows);
    colX = edges(colWidths, HEADER_WIDTH);
    rowY = edges(rowHeights, HEADER_HEIGHT);
    charts = await engine.charts(sheet);
    validations = await engine.validations(sheet);
    pivotPanel.setKnown(await engine.pivots(sheet));
    if (!charts.some((chart) => chart.id === selectedChart)) selectedChart = null;
    cells = new Map(view.cells.map((cell) => [key(cell.col, cell.row), cell]));
    merges = view.merges;
    covered = new Set();
    for (const merge of merges) {
      for (let col = merge.col; col < merge.col + merge.cols; col++) {
        for (let row = merge.row; row < merge.row + merge.rows; row++) {
          if (col !== merge.col || row !== merge.row) covered.add(key(col, row));
        }
      }
    }
  } catch (error) {
    statusEl.textContent = `Erro ao ler a planilha: ${error}`;
  }
  draw();
  syncFormulaBar();
  syncToolbar();
}

/** Rola o mínimo necessário para o foco da seleção ficar inteiro na tela. */
async function reveal() {
  const { focus, origin } = pos;
  // O que está congelado já está sempre na tela.
  if (focus.col >= frozen.cols) origin.col = Math.min(origin.col, focus.col);
  if (focus.row >= frozen.rows) origin.row = Math.min(origin.row, focus.row);
  await refresh();
  // As larguras variam, então avança uma coluna ou linha por vez e confere.
  for (let guard = 0; guard < 1000; guard++) {
    const colIndex = colBand(focus.col);
    const rowIndex = rowBand(focus.row);
    const colCut = focus.col > origin.col && (colIndex >= colWidths.length || colX[colIndex + 1] > wrap.clientWidth);
    const rowCut = focus.row > origin.row && (rowIndex >= rowHeights.length || rowY[rowIndex + 1] > wrap.clientHeight);
    if (!colCut && !rowCut) break;
    // Um destino longe (o de uma busca, por exemplo) vai direto para perto do
    // começo da tela, com um pouco do que vem antes; de perto, um passo por vez.
    if (colCut) origin.col = colIndex >= colWidths.length ? Math.max(origin.col + 1, focus.col - 1) : origin.col + 1;
    if (rowCut) origin.row = rowIndex >= rowHeights.length ? Math.max(origin.row + 1, focus.row - 3) : origin.row + 1;
    await refresh();
  }
}

/** Faz a seleção de uma célula só coincidir com a área mesclada em que ela caiu. */
function snapToMerge() {
  const block = blockOf(pos.anchor);
  pos.anchor = { col: block.col, row: block.row };
  pos.focus = { col: block.col + block.cols - 1, row: block.row + block.rows - 1 };
}

/** Move o foco da seleção; sem `extend`, a seleção vira essa única célula. */
async function select(cell: Cell, extend = false) {
  pos.focus = { col: clamp(cell.col, MAX_COLS - 1), row: clamp(cell.row, MAX_ROWS - 1) };
  if (!extend) {
    pos.anchor = { ...pos.focus };
    snapToMerge();
  }
  await reveal();
  // A célula podia estar fora da tela, e só agora se sabe se ela é mesclada.
  if (!extend && mergeAt(pos.anchor)) {
    snapToMerge();
    draw();
    syncFormulaBar();
    syncToolbar();
  }
}

/**
 * Anda um passo com a célula ativa. `step` só pula as linhas e colunas
 * ocultas que estão na tela; se o destino ainda for oculto, segue adiante.
 */
async function move(dx: number, dy: number) {
  for (let guard = 0; guard < 100; guard++) {
    const before = { ...pos.anchor };
    await select(step(dx, dy));
    const { anchor } = pos;
    const hidden = (dx !== 0 && colWidths[colBand(anchor.col)] === 0) || (dy !== 0 && rowHeights[rowBand(anchor.row)] === 0);
    if (!hidden) return;
    // Não há nada visível nesse sentido: fica onde estava.
    if (anchor.col === before.col && anchor.row === before.row) return;
  }
}

const DIRECTIONS: Record<string, Direction> = { ArrowLeft: "left", ArrowRight: "right", ArrowUp: "up", ArrowDown: "down" };

/**
 * Ctrl+seta salta até a borda do bloco de dados; Ctrl+Home vai a A1 e
 * Ctrl+End à última célula preenchida. Com Shift a seleção cresce até lá.
 */
async function jumpTo(keyName: string, extend: boolean) {
  try {
    const from = extend ? pos.focus : pos.anchor;
    let target: Cell;
    if (keyName === "Home") target = { col: 0, row: 0 };
    else if (keyName === "End") target = await engine.lastCell(sheet);
    else target = await engine.jump(sheet, from.col, from.row, DIRECTIONS[keyName]);
    await select(target, extend);
  } catch (error) {
    statusEl.textContent = `Erro ao mover: ${error}`;
  }
}

/** Home vai ao começo da linha; PageUp e PageDown andam uma tela, rolando junto. */
async function pageTo(keyName: string, extend: boolean) {
  const from = extend ? pos.focus : pos.anchor;
  if (keyName === "Home") {
    await select({ col: 0, row: from.row }, extend);
    return;
  }
  const rows = Math.max(1, rowHeights.length - frozen.rows - 1);
  const dir = keyName === "PageDown" ? 1 : -1;
  pos.origin.row = Math.max(frozen.rows, clamp(pos.origin.row + dir * rows, MAX_ROWS - 1));
  await select({ col: from.col, row: clamp(from.row + dir * rows, MAX_ROWS - 1) }, extend);
}

/** Grava o texto da barra de fórmulas na célula ativa. */
async function write(): Promise<boolean> {
  const { anchor } = pos;
  const current = cells.get(key(anchor.col, anchor.row))?.input ?? "";
  if (!dirty || fx.value === current) {
    dirty = false;
    stopEdit();
    return true;
  }
  try {
    await engine.setCell(sheet, anchor.col, anchor.row, fx.value);
    dirty = false;
    stopEdit();
    copiedMark = null;
    statusEl.textContent = "Pronto";
    markModified();
    return true;
  } catch (error) {
    const text = String(error);
    if (text.startsWith("Validação: ")) {
      statusEl.textContent = text;
      await engine.alert(text.slice("Validação: ".length));
    } else {
      statusEl.textContent = `Erro ao gravar a célula: ${text}`;
    }
    return false;
  }
}

/** Grava a edição e move a célula ativa. */
async function commit(moveCols: number, moveRows: number) {
  const edited = { ...pos.anchor };
  if (!(await write())) return;
  canvas.focus();
  await move(moveCols, moveRows);
  await fitRow(edited);
  await fitColumn(edited);
}

/** Aplica uma propriedade de estilo à seleção. */
async function format(property: StyleProperty, value: string) {
  if (!(await write())) return;
  try {
    await engine.format(sheet, selectedRange(), property, value);
    statusEl.textContent = "Pronto";
    markModified();
  } catch (error) {
    statusEl.textContent = `Erro ao formatar: ${error}`;
  }
  canvas.focus();
  await refresh();
}

/**
 * Aumenta a altura da linha se o conteúdo de uma célula não couber nela (por
 * quebrar em várias linhas ou por ter fonte grande), como o Excel faz. Nunca
 * diminui: a altura pode ter sido escolhida à mão.
 */
async function fitRow(cell: Cell) {
  const view = cells.get(key(cell.col, cell.row));
  const rowIndex = rowBand(cell.row);
  if (!view || view.display === "" || mergeAt(cell) || !rowOnScreen(cell.row) || rowHeights[rowIndex] === 0) return;
  const wraps = view.style?.wrap === true && view.kind === "text";
  ctx.font = cellFont(view);
  const width = xOf(cell.col + 1) - xOf(cell.col) - 2 * PADDING;
  const lines = wraps ? wrapLines(view.display, width).length : 1;
  const needed = lines * Math.round(LINE_HEIGHT * fontScale(view)) + 8;
  if (needed <= rowHeights[rowIndex]) return;
  try {
    await engine.setRowHeight(sheet, cell.row, Math.min(needed, 2000));
  } catch (error) {
    statusEl.textContent = `Erro ao ajustar a altura da linha: ${error}`;
  }
  await refresh();
}

/**
 * Alarga a coluna se um número digitado não couber nela e ela ainda tiver a
 * largura padrão, como o Excel faz; assim o número não vira uma fileira de
 * "#". Colunas que a pessoa ajustou à mão ficam como estão.
 */
async function fitColumn(cell: Cell) {
  const view = cells.get(key(cell.col, cell.row));
  if (!view || view.kind !== "number" || mergeAt(cell) || !colOnScreen(cell.col)) return;
  const width = colWidths[colBand(cell.col)];
  if (width !== DEFAULT_COL_WIDTH) return;
  ctx.font = cellFont(view);
  const needed = Math.ceil(ctx.measureText(view.display).width) + 2 * PADDING + 2;
  if (needed <= width) return;
  try {
    await engine.setColWidth(sheet, cell.col, Math.min(needed, 600));
  } catch (error) {
    statusEl.textContent = `Erro ao ajustar a largura da coluna: ${error}`;
  }
  await refresh();
}

/** Faz uma alteração na planilha que não é digitar numa célula, e redesenha. */
async function change(label: string, action: () => Promise<void>) {
  if (!(await write())) return;
  try {
    await action();
    copiedMark = null;
    statusEl.textContent = "Pronto";
    markModified();
  } catch (error) {
    statusEl.textContent = `Erro ao ${label}: ${error}`;
  }
  canvas.focus();
  await refresh();
}

/** Mescla a seleção, ou desfaz a mesclagem se a célula ativa já é mesclada. */
async function toggleMerge() {
  const range = selectedRange();
  const merged = mergeAt(pos.anchor) !== undefined;
  await change(merged ? "desfazer a mesclagem" : "mesclar", () =>
    merged ? engine.unmerge(sheet, range) : engine.merge(sheet, range),
  );
  // Depois de mesclar, a seleção passa a ser a célula nova.
  pos.anchor = { col: range.col, row: range.row };
  pos.focus = { ...pos.anchor };
  snapToMerge();
  draw();
  syncFormulaBar();
  syncToolbar();
}

/** Insere ou exclui linhas ou colunas inteiras, na medida da seleção. */
function changeStructure(action: string) {
  const range = selectedRange();
  const plans: Record<string, [Structure, number, number, string]> = {
    "insert-row-above": ["insertRows", range.row, range.rows, "inserir linhas"],
    "insert-row-below": ["insertRows", range.row + range.rows, range.rows, "inserir linhas"],
    "delete-rows": ["deleteRows", range.row, range.rows, "excluir linhas"],
    "insert-col-left": ["insertCols", range.col, range.cols, "inserir colunas"],
    "insert-col-right": ["insertCols", range.col + range.cols, range.cols, "inserir colunas"],
    "delete-cols": ["deleteCols", range.col, range.cols, "excluir colunas"],
  };
  const [structure, index, count, label] = plans[action];
  return change(label, () => engine.changeStructure(sheet, structure, index, count));
}

/** Liga ou desliga um estilo da barra de ferramentas, a partir da célula ativa. */
async function toggle(property: ToggleProperty) {
  const cell = { ...pos.anchor };
  const current = cells.get(key(cell.col, cell.row))?.style?.[property] ?? false;
  await format(property, String(!current));
  if (property === "wrap" && !current) await fitRow(cell);
}

/** Preenche da seleção até o destino da alça e seleciona tudo o que foi preenchido. */
async function fillSelection(target: { axis: "rows" | "cols"; to: number }) {
  const source = selectedRange();
  const filled = fillRange();
  try {
    await engine.fill(sheet, source, target.axis, target.to);
    pos.anchor = { col: filled.col, row: filled.row };
    pos.focus = { col: filled.col + filled.cols - 1, row: filled.row + filled.rows - 1 };
    copiedMark = null;
    statusEl.textContent = "Pronto";
    markModified();
  } catch (error) {
    statusEl.textContent = `Erro ao preencher: ${error}`;
  }
  await refresh();
}

/** Copia ou recorta a seleção para a área de transferência. */
async function copySelection(cut: boolean) {
  if (!(await write())) return;
  const range = selectedRange();
  try {
    await engine.copy(sheet, range, cut);
    copiedMark = { sheet, range, cut };
    statusEl.textContent = cut ? "Recortado: cole no destino" : "Copiado";
  } catch (error) {
    statusEl.textContent = `Erro ao copiar: ${error}`;
  }
  draw();
}

/** Cola a área de transferência a partir da célula ativa e seleciona o que entrou. */
async function pasteAtSelection() {
  if (!(await write())) return;
  try {
    const pasted = await engine.paste(sheet, pos.anchor.col, pos.anchor.row);
    if (pasted === null) {
      statusEl.textContent = "Não há nada para colar";
      return;
    }
    // O que foi recortado só cola uma vez.
    if (copiedMark?.cut) copiedMark = null;
    pos.anchor = { col: pasted.col, row: pasted.row };
    pos.focus = { col: pasted.col + pasted.cols - 1, row: pasted.row + pasted.rows - 1 };
    statusEl.textContent = "Pronto";
    markModified();
  } catch (error) {
    statusEl.textContent = `Erro ao colar: ${error}`;
  }
  await refresh();
}

/** Apaga o conteúdo da seleção. */
async function clearSelection() {
  dirty = false;
  try {
    await engine.clearCells(sheet, selectedRange());
    copiedMark = null;
    markModified();
  } catch (error) {
    statusEl.textContent = `Erro ao apagar: ${error}`;
  }
  await refresh();
}

/** Refaz a busca do "Localizar" e redesenha as marcas. */
async function runFind() {
  const query = findBar.hidden ? "" : findQuery.value;
  try {
    found = query === "" ? [] : await engine.find(sheet, query);
  } catch (error) {
    found = [];
    statusEl.textContent = `Erro ao localizar: ${error}`;
  }
  foundKeys = new Set(found.map((cell) => key(cell.col, cell.row)));
  syncFindCount();
  draw();
}

/** Mostra qual das células encontradas é a ativa, e quantas são. */
function syncFindCount() {
  const current = found.findIndex((cell) => cell.col === pos.anchor.col && cell.row === pos.anchor.row);
  if (findQuery.value === "") findCount.textContent = "";
  else if (found.length === 0) findCount.textContent = "Nada encontrado";
  else findCount.textContent = current < 0 ? `${found.length} encontradas` : `${current + 1} de ${found.length}`;
}

/** A célula vem antes de outra na ordem de leitura. */
const comesBefore = (a: Cell, b: Cell) => a.row < b.row || (a.row === b.row && a.col < b.col);

/**
 * Vai para a célula encontrada seguinte (ou anterior) à ativa, dando a volta
 * no fim. Com `stay`, a própria célula ativa serve, se for uma das encontradas.
 */
async function findStep(forward: boolean, stay = false) {
  if (found.length === 0) return;
  const { anchor } = pos;
  let target: Cell | undefined;
  if (stay) target = found.find((cell) => cell.col === anchor.col && cell.row === anchor.row);
  if (!target && forward) target = found.find((cell) => comesBefore(anchor, cell)) ?? found[0];
  if (!target && !forward) target = [...found].reverse().find((cell) => comesBefore(cell, anchor)) ?? found[found.length - 1];
  if (!target) return;
  await select(target);
  syncFindCount();
}

function openFind() {
  findBar.hidden = false;
  findQuery.focus();
  findQuery.select();
  void runFind();
}

function closeFind() {
  findBar.hidden = true;
  found = [];
  foundKeys = new Set();
  canvas.focus();
  draw();
}

/** Troca o texto procurado na célula ativa, se ela for uma das encontradas, e segue para a próxima. */
async function replaceCurrent() {
  if (!(await write())) return;
  const { anchor } = pos;
  if (!foundKeys.has(key(anchor.col, anchor.row))) {
    await findStep(true, true);
    return;
  }
  let status = "Substituído";
  try {
    const replaced = await engine.replace(sheet, anchor.col, anchor.row, findQuery.value, replaceText.value);
    if (replaced) {
      copiedMark = null;
      modified = true;
      syncDocument();
    } else {
      status = "O texto está no resultado da fórmula, não no que foi digitado";
    }
  } catch (error) {
    status = `Erro ao substituir: ${error}`;
  }
  await refresh();
  await runFind();
  await findStep(true);
  statusEl.textContent = status;
}

async function replaceEverything() {
  if (!(await write())) return;
  let status: string;
  try {
    const changed = await engine.replaceAll(sheet, findQuery.value, replaceText.value);
    if (changed > 0) {
      copiedMark = null;
      modified = true;
      syncDocument();
    }
    status = changed === 1 ? "1 célula alterada" : `${changed} células alteradas`;
  } catch (error) {
    status = `Erro ao substituir: ${error}`;
  }
  await refresh();
  await runFind();
  statusEl.textContent = status;
}

/** Congela as linhas acima e as colunas à esquerda da seleção. */
async function freezeAtSelection() {
  if (!(await write())) return;
  const range = selectedRange();
  // Uma coluna inteira selecionada congela só colunas; uma linha inteira, só linhas.
  const cols = range.cols === MAX_COLS ? 0 : range.col;
  const rows = range.rows === MAX_ROWS ? 0 : range.row;
  if (cols === 0 && rows === 0) {
    statusEl.textContent = "Selecione a célula abaixo e à direita do que deve ficar fixo";
    return;
  }
  try {
    await engine.setFrozen(sheet, cols, rows);
    pos.origin = { col: cols, row: rows };
    await refresh();
    // Sem espaço para a parte que rola, a planilha ficaria presa.
    const room = (bounds: number[], count: number, size: number) => size - bounds[Math.min(count, bounds.length - 1)];
    if (room(colX, cols, wrap.clientWidth) < 60 || room(rowY, rows, wrap.clientHeight) < 30) {
      await engine.undo();
      statusEl.textContent = "Não cabe congelar tanto nesta janela";
    } else {
      statusEl.textContent = "Painéis congelados";
      markModified();
    }
  } catch (error) {
    statusEl.textContent = `Erro ao congelar: ${error}`;
  }
  canvas.focus();
  await refresh();
}

/** Oculta ou reexibe as linhas ou colunas da seleção. */
async function setHidden(axis: "rows" | "cols", hidden: boolean) {
  const range = selectedRange();
  const [first, count] = axis === "rows" ? [range.row, range.rows] : [range.col, range.cols];
  await change(hidden ? "ocultar" : "reexibir", () => engine.setHidden(sheet, axis, first, count, hidden));
  const { anchor } = pos;
  const gone = axis === "cols" ? colWidths[colBand(anchor.col)] === 0 : rowHeights[rowBand(anchor.row)] === 0;
  // A célula ativa ficou oculta: passa para a seguinte à mostra.
  if (hidden && gone) {
    pos.focus = { ...anchor };
    await move(axis === "cols" ? 1 : 0, axis === "rows" ? 1 : 0);
  }
}

/** Classifica pela coluna da célula ativa: a seleção, ou a tabela em volta dela. */
async function sortSelection(descending: boolean) {
  if (!(await write())) return;
  const { anchor } = pos;
  let range = selectedRange();
  // Com a coluna inteira selecionada, vale a tabela que começa no topo dela.
  if (range.rows === MAX_ROWS || range.cols === MAX_COLS) range = { col: anchor.col, row: anchor.row, cols: 1, rows: 1 };
  let status: string;
  try {
    const sorted = await engine.sort(sheet, range, anchor.col, descending);
    const last = { col: sorted.col + sorted.cols - 1, row: sorted.row + sorted.rows - 1 };
    copiedMark = null;
    markModified();
    status = `Classificado: ${cellName(sorted)}:${cellName(last)}`;
  } catch (error) {
    status = `Erro ao classificar: ${error}`;
  }
  canvas.focus();
  await refresh();
  statusEl.textContent = status;
}

/** Abre o painel com os valores da coluna da célula ativa, para escolher quais mostrar. */
async function openFilter(left: number, top: number) {
  if (!(await write())) return;
  try {
    filtering = await engine.filterOptions(sheet, pos.anchor.col, pos.anchor.row);
  } catch (error) {
    statusEl.textContent = `Erro ao filtrar: ${error}`;
    return;
  }
  filterTitle.textContent = `Filtrar a coluna ${colName(filtering.col)}`;
  filterValues.replaceChildren(
    ...filtering.values.map((value) => {
      const label = document.createElement("label");
      const box = document.createElement("input");
      box.type = "checkbox";
      box.checked = value.shown;
      box.value = value.text;
      label.append(box, value.text === "" ? "(Vazias)" : value.text);
      return label;
    }),
  );
  filterPanel.hidden = false;
  filterPanel.style.left = `${Math.max(0, Math.min(left, window.innerWidth - filterPanel.offsetWidth - 8))}px`;
  filterPanel.style.top = `${Math.max(0, Math.min(top, window.innerHeight - filterPanel.offsetHeight - 8))}px`;
  filterValues.querySelector<HTMLInputElement>("input")?.focus();
}

function closeFilter() {
  filterPanel.hidden = true;
  filtering = null;
  canvas.focus();
}

/** Mostra só as linhas com os valores marcados no painel. */
async function applyFilter() {
  const options = filtering;
  if (!options) return;
  const boxes = [...filterValues.querySelectorAll<HTMLInputElement>("input")];
  const allowed = boxes.filter((box) => box.checked).map((box) => box.value);
  closeFilter();
  let status: string;
  try {
    const hidden = await engine.applyFilter(sheet, options.range, options.col, allowed);
    copiedMark = null;
    markModified();
    status = hidden === 0 ? "Filtro: todas as linhas à mostra" : `Filtro: ${hidden} ${hidden === 1 ? "linha oculta" : "linhas ocultas"}`;
  } catch (error) {
    status = `Erro ao filtrar: ${error}`;
  }
  await refresh();
  // A célula ativa pode ter ficado oculta.
  if (rowOnScreen(pos.anchor.row) && rowHeights[rowBand(pos.anchor.row)] === 0) await move(0, 1);
  statusEl.textContent = status;
}

/** Reexibe todas as linhas da tabela em volta da célula ativa. */
async function clearFilter() {
  if (!(await write())) return;
  let status = "Filtro: todas as linhas à mostra";
  try {
    const { range } = await engine.filterOptions(sheet, pos.anchor.col, pos.anchor.row);
    await engine.setHidden(sheet, "rows", range.row, range.rows, false);
    markModified();
  } catch (error) {
    status = `Erro ao limpar o filtro: ${error}`;
  }
  canvas.focus();
  await refresh();
  statusEl.textContent = status;
}

/** Desfaz ou refaz a última edição e redesenha. */
async function history(action: "undo" | "redo") {
  try {
    await engine[action]();
  } catch (error) {
    statusEl.textContent = `Erro ao ${action === "undo" ? "desfazer" : "refazer"}: ${error}`;
    return;
  }
  markModified();
  // Desfazer pode criar ou remover uma planilha.
  await loadSheets();
  await refresh();
}

/** Último clique numa aba, para reconhecer o clique duplo. */
let lastTabClick = { index: -1, time: 0 };

/**
 * Trata o clique numa aba: o primeiro mostra a planilha e o segundo, logo em
 * seguida, renomeia. O evento `dblclick` não serve aqui porque o primeiro
 * clique redesenha as abas e o segundo cai num elemento novo.
 */
function clickTab(tab: HTMLElement, info: SheetInfo) {
  const now = performance.now();
  const double = lastTabClick.index === info.index && now - lastTabClick.time < 500;
  lastTabClick = { index: double ? -1 : info.index, time: now };
  if (double) renameTab(tab, info);
  else if (info.index !== sheet) void showSheet(info.index);
}

/** Redesenha as abas de planilha. */
function renderTabs() {
  const items: HTMLElement[] = [];
  for (const info of sheets) {
    const tab = document.createElement("button");
    tab.type = "button";
    tab.setAttribute("role", "tab");
    tab.setAttribute("aria-selected", String(info.index === sheet));
    tab.title = "Clique duas vezes para renomear";
    tab.textContent = info.name;
    tab.addEventListener("click", () => clickTab(tab, info));
    items.push(tab);
    if (info.index === sheet && sheets.length > 1) {
      const remove = document.createElement("button");
      remove.type = "button";
      remove.className = "remove-sheet";
      remove.textContent = "×";
      remove.title = "Excluir planilha";
      remove.setAttribute("aria-label", `Excluir a planilha ${info.name}`);
      remove.addEventListener("click", () => void deleteSheet(info));
      items.push(remove);
    }
  }
  sheetTabs.replaceChildren(...items);
}

/** Troca a aba por um campo de texto para renomear a planilha. */
function renameTab(tab: HTMLElement, info: SheetInfo) {
  const input = document.createElement("input");
  input.value = info.name;
  input.setAttribute("aria-label", `Novo nome da planilha ${info.name}`);
  // Enter e a perda de foco gravam; Esc cancela. `finish` roda uma vez só.
  let finished = false;
  const finish = async (save: boolean) => {
    if (finished) return;
    finished = true;
    const name = input.value.trim();
    if (save && name !== "" && name !== info.name) {
      try {
        await engine.renameSheet(info.index, name);
        statusEl.textContent = "Pronto";
        markModified();
      } catch (error) {
        statusEl.textContent = `Erro ao renomear a planilha: ${error}`;
      }
    }
    await loadSheets();
    canvas.focus();
    // Fórmulas de outras planilhas podem citar o nome antigo.
    await refresh();
  };
  input.addEventListener("keydown", (event) => {
    if (event.key === "Enter") void finish(true);
    else if (event.key === "Escape") void finish(false);
  });
  input.addEventListener("blur", () => void finish(true));
  tab.replaceWith(input);
  input.focus();
  input.select();
}

async function deleteSheet(info: SheetInfo) {
  if (!(await write())) return;
  try {
    const confirmed = await engine.confirm(`Excluir a planilha "${info.name}" e todo o conteúdo dela?`, "Excluir planilha");
    if (!confirmed) return;
    await engine.deleteSheet(info.index);
    statusEl.textContent = "Pronto";
    markModified();
  } catch (error) {
    statusEl.textContent = `Erro ao excluir a planilha: ${error}`;
    return;
  }
  // Os índices das planilhas seguintes mudaram, então as posições guardadas não valem mais.
  copiedMark = null;
  positions = new Map();
  pos = newPosition();
  await loadSheets();
  canvas.focus();
  await refresh();
}

/** Lê do motor a lista de planilhas e garante que a atual ainda existe. */
async function loadSheets() {
  try {
    sheets = await engine.sheets();
  } catch (error) {
    statusEl.textContent = `Erro ao ler as planilhas: ${error}`;
    return;
  }
  if (!sheets.some((info) => info.index === sheet) && sheets.length > 0) {
    sheet = sheets[0].index;
    pos = positions.get(sheet) ?? newPosition();
  }
  renderTabs();
}

/** Troca a planilha na tela, guardando a posição da anterior. */
async function showSheet(index: number) {
  if (!(await write())) return;
  positions.set(sheet, pos);
  selectedChart = null;
  closeChartPanel();
  sheet = index;
  pos = positions.get(index) ?? newPosition();
  renderTabs();
  canvas.focus();
  await refresh();
}

async function addSheet() {
  if (!(await write())) return;
  try {
    const index = await engine.addSheet();
    markModified();
    await loadSheets();
    await showSheet(index);
  } catch (error) {
    statusEl.textContent = `Erro ao criar a planilha: ${error}`;
  }
}

/**
 * Resolve as alterações não salvas antes de uma ação que as perderia: pergunta
 * se salva, descarta ou cancela. Devolve se a ação pode seguir.
 */
async function settleChanges(): Promise<boolean> {
  if (!modified) return true;
  const choice = await engine.askToSave(docName.textContent ?? "Sem título");
  if (choice === "cancel") return false;
  if (choice === "save") await saveFile(false);
  // Salvar pode ter sido cancelado na escolha do arquivo, ou ter falhado.
  return choice === "discard" || !modified;
}

async function openFile() {
  const files = engine.files;
  if (!files) return;
  if (!(await write())) return;
  try {
    if (!(await settleChanges())) return;
    const path = await files.open();
    if (path === null) return;
    // Um CSV vira uma pasta nova: salvar pede um nome .xlsx, e o CSV de
    // origem fica como estava.
    const imported = path.toLowerCase().endsWith(".csv");
    filePath = imported ? null : path;
    modified = false;
    copiedMark = null;
    positions = new Map();
    sheet = 0;
    pos = newPosition();
    selectedChart = null;
    closeChartPanel();
    statusEl.textContent = "Pronto";
    syncDocument();
    await loadSheets();
    await refresh();
  } catch (error) {
    statusEl.textContent = `Erro ao abrir o arquivo: ${error}`;
  }
  canvas.focus();
}

async function saveFile(askWhere: boolean) {
  const files = engine.files;
  if (!files) return;
  if (!(await write())) return;
  try {
    const path = await files.save(askWhere ? null : filePath, sheet);
    if (path === null) return;
    if (path.toLowerCase().endsWith(".csv")) {
      // Exportar não troca o documento aberto, e o CSV leva só a planilha atual.
      statusEl.textContent = "Planilha exportada como CSV (só os valores, sem formatação)";
    } else {
      filePath = path;
      modified = false;
      statusEl.textContent = "Arquivo salvo";
    }
    syncDocument();
  } catch (error) {
    statusEl.textContent = `Erro ao salvar o arquivo: ${error}`;
  }
  canvas.focus();
}

/** Posição de um evento do mouse dentro do canvas. */
function pointerAt(event: MouseEvent) {
  const bounds = canvas.getBoundingClientRect();
  return { px: event.clientX - bounds.left, py: event.clientY - bounds.top };
}

/** Grava no motor o tamanho de uma coluna ou linha da tela. */
async function resize(grip: Grip, size: number) {
  try {
    if (grip.kind === "col") await engine.setColWidth(sheet, bandCol(grip.index), size);
    else await engine.setRowHeight(sheet, bandRow(grip.index), size);
    statusEl.textContent = "Pronto";
    markModified();
  } catch (error) {
    statusEl.textContent = `Erro ao redimensionar: ${error}`;
  }
  await refresh();
}

/** Cabeçalho sob um ponto do canvas: de coluna, de linha, o canto, ou nenhum. */
function headerAt(px: number, py: number): "col" | "row" | "corner" | null {
  if (px < HEADER_WIDTH && py < HEADER_HEIGHT) return "corner";
  if (py < HEADER_HEIGHT) return "col";
  return px < HEADER_WIDTH ? "row" : null;
}

/** Seleciona colunas ou linhas inteiras, de `from` até `to`, ou a planilha toda. */
function selectHeaders(kind: "col" | "row" | "corner", from: Cell, to: Cell) {
  if (kind === "corner") {
    pos.anchor = { col: 0, row: 0 };
    pos.focus = { col: MAX_COLS - 1, row: MAX_ROWS - 1 };
  } else if (kind === "col") {
    pos.anchor = { col: from.col, row: 0 };
    pos.focus = { col: to.col, row: MAX_ROWS - 1 };
  } else {
    pos.anchor = { col: 0, row: from.row };
    pos.focus = { col: MAX_COLS - 1, row: to.row };
  }
  draw();
  syncFormulaBar();
  syncToolbar();
}

/** Leva o arrasto em andamento (seleção, cabeçalhos, alça ou fórmula) até um ponto. */
function dragTo(px: number, py: number) {
  const cell = cellAt(px, py);
  if (fillDrag) {
    // Preenche no sentido em que o mouse mais se afastou da seleção.
    const range = selectedRange();
    const outside = (value: number, start: number, size: number) =>
      value < start ? value - start : value >= start + size ? value - (start + size - 1) : 0;
    const rows = outside(cell.row, range.row, range.rows);
    const cols = outside(cell.col, range.col, range.cols);
    if (rows === 0 && cols === 0) fillTarget = null;
    else if (Math.abs(rows) >= Math.abs(cols)) fillTarget = { axis: "rows", to: cell.row };
    else fillTarget = { axis: "cols", to: cell.col };
    draw();
  } else if (pointDrag) {
    if (pointing) pointAt(pointing.input, pointing.from, cell);
  } else if (headerDrag) {
    selectHeaders(headerDrag.kind, headerDrag.from, cell);
  } else if (dragging && (cell.col !== pos.focus.col || cell.row !== pos.focus.row)) {
    pos.focus = cell;
    draw();
    syncFormulaBar();
  }
}

/** Para onde rolar quando o mouse, num arrasto, passa da borda da grade. */
function edgeDirection() {
  const { px, py } = lastPointer;
  const dx = px > wrap.clientWidth - 4 ? 1 : px < HEADER_WIDTH ? -1 : 0;
  const dy = py > wrap.clientHeight - 4 ? 1 : py < HEADER_HEIGHT ? -1 : 0;
  // Arrastando cabeçalhos, o mouse fica sobre eles: só o outro eixo rola.
  if (headerDrag?.kind === "col") return { dx, dy: 0 };
  if (headerDrag?.kind === "row") return { dx: 0, dy };
  return { dx, dy };
}

function stopAutoScroll() {
  if (scrollTimer !== null) window.clearInterval(scrollTimer);
  scrollTimer = null;
}

/** Rola a grade aos poucos enquanto um arrasto mantém o mouse além da borda. */
function autoScroll() {
  const { dx, dy } = edgeDirection();
  if (dx === 0 && dy === 0) {
    stopAutoScroll();
    return;
  }
  if (scrollTimer !== null) return;
  scrollTimer = window.setInterval(() => {
    const step = edgeDirection();
    const { origin } = pos;
    const col = Math.max(frozen.cols, clamp(origin.col + step.dx, MAX_COLS - 1));
    const row = Math.max(frozen.rows, clamp(origin.row + step.dy, MAX_ROWS - 1));
    if (scrolling || (col === origin.col && row === origin.row)) return;
    scrolling = true;
    origin.col = col;
    origin.row = row;
    void refresh().then(() => {
      scrolling = false;
      dragTo(lastPointer.px, lastPointer.py);
    });
  }, 60);
}

// --- Gráficos ---

/** Move ou redimensiona o gráfico segundo o mouse, de célula em célula. */
function dragChart(px: number, py: number) {
  const drag = chartDrag;
  const chart = drag && charts.find((one) => one.id === drag.id);
  if (!drag || !chart) return;
  const cell = cellAt(px, py);
  const { origin } = drag;
  if (drag.mode === "move") {
    const cols = origin.toCol - origin.fromCol;
    const rows = origin.toRow - origin.fromRow;
    chart.fromCol = clamp(origin.fromCol + cell.col - drag.grab.col, MAX_COLS - 1 - cols);
    chart.fromRow = clamp(origin.fromRow + cell.row - drag.grab.row, MAX_ROWS - 1 - rows);
    chart.toCol = chart.fromCol + cols;
    chart.toRow = chart.fromRow + rows;
  } else {
    chart.toCol = Math.max(cell.col, origin.fromCol + 2);
    chart.toRow = Math.max(cell.row, origin.fromRow + 3);
  }
  drag.moved = true;
  draw();
}

/** Grava o novo estado do gráfico; se o motor recusar, volta ao que estava. */
async function saveChart(chart: ChartInfo) {
  try {
    const saved = await engine.updateChart(chart);
    charts = charts.map((one) => (one.id === saved.id ? saved : one));
    statusEl.textContent = "Pronto";
    markModified();
    chartError.hidden = true;
  } catch (error) {
    statusEl.textContent = `Erro ao alterar o gráfico: ${error}`;
    chartError.textContent = String(error);
    chartError.hidden = chartPanel.hidden;
  }
  await refresh();
}

async function removeChart(id: number) {
  try {
    await engine.deleteChart(id);
    selectedChart = null;
    closeChartPanel();
    statusEl.textContent = "Pronto";
    markModified();
  } catch (error) {
    statusEl.textContent = `Erro ao apagar o gráfico: ${error}`;
  }
  canvas.focus();
  await refresh();
}

/** Cria um gráfico com a seleção, ou com a tabela em volta da célula ativa. */
async function insertChart(kind: ChartKind) {
  if (!(await write())) return;
  try {
    let range = selectedRange();
    if (range.cols === 1 && range.rows === 1) range = await engine.dataRegion(sheet, range.col, range.row);
    if (range.cols * range.rows < 2) {
      statusEl.textContent = "Selecione as células com os dados do gráfico";
      return;
    }
    const created = await engine.addChart(sheet, kind, range);
    selectedChart = created.id;
    markModified();
    statusEl.textContent = "Gráfico inserido: arraste para mover; dois cliques abrem as opções";
  } catch (error) {
    statusEl.textContent = `Erro ao inserir o gráfico: ${error}`;
  }
  canvas.focus();
  await refresh();
}

const rangeText = (range: Range) =>
  `${colName(range.col)}${range.row + 1}:${colName(range.col + range.cols - 1)}${range.row + range.rows}`;

/** Lê `A1:C5` (ou `A1`) como intervalo. */
function parseRange(text: string): Range | null {
  const match = /^\s*\$?([A-Za-z]{1,3})\$?(\d{1,7})(?::\$?([A-Za-z]{1,3})\$?(\d{1,7}))?\s*$/.exec(text);
  if (!match) return null;
  const letters = (name: string) => [...name.toUpperCase()].reduce((col, letter) => col * 26 + letter.charCodeAt(0) - 64, 0) - 1;
  const [, c1, r1, c2 = c1, r2 = r1] = match;
  const cols = [letters(c1), letters(c2)].sort((a, b) => a - b);
  const rows = [Number(r1) - 1, Number(r2) - 1].sort((a, b) => a - b);
  if (cols[1] >= MAX_COLS || rows[0] < 0 || rows[1] >= MAX_ROWS) return null;
  return { col: cols[0], row: rows[0], cols: cols[1] - cols[0] + 1, rows: rows[1] - rows[0] + 1 };
}

/** O gráfico que o painel de opções edita. */
let panelChart: number | null = null;

function openChartPanel(chart: ChartInfo) {
  panelChart = chart.id;
  chartTitle.value = chart.title;
  chartKindSelect.value = chart.kind;
  chartRange.value = rangeText(chart.range);
  chartError.hidden = true;
  chartPanel.hidden = false;
  const area = canvas.getBoundingClientRect();
  const box = chartBox(chart);
  const width = chartPanel.offsetWidth;
  const height = chartPanel.offsetHeight;
  let left = area.left + (box ? box.left + box.width + 10 : 60);
  if (left + width > window.innerWidth - 8) left = area.left + (box ? box.left : 60) - width - 10;
  const top = area.top + (box ? box.top : 60);
  chartPanel.style.left = `${Math.max(8, Math.min(left, window.innerWidth - width - 8))}px`;
  chartPanel.style.top = `${Math.max(8, Math.min(top, window.innerHeight - height - 8))}px`;
}

function closeChartPanel() {
  chartPanel.hidden = true;
  panelChart = null;
}

/** Aplica uma mudança do painel ao gráfico dele. */
async function changeChart(patch: (chart: ChartInfo) => ChartInfo | null) {
  const chart = charts.find((one) => one.id === panelChart);
  if (!chart) return;
  const updated = patch({ ...chart });
  if (updated) await saveChart(updated);
}

chartTitle.addEventListener("change", () => void changeChart((chart) => ({ ...chart, title: chartTitle.value.trim() })));
chartKindSelect.addEventListener("change", () => void changeChart((chart) => ({ ...chart, kind: chartKindSelect.value as ChartKind })));
chartRange.addEventListener("change", () =>
  void changeChart((chart) => {
    const range = parseRange(chartRange.value);
    if (!range) {
      chartError.textContent = "Escreva o intervalo assim: A1:C5";
      chartError.hidden = false;
      return null;
    }
    return { ...chart, range };
  }),
);
$("#chart-delete").addEventListener("click", () => {
  if (panelChart !== null) void removeChart(panelChart);
});
$("#chart-close").addEventListener("click", () => {
  closeChartPanel();
  canvas.focus();
});
chartPanel.addEventListener("keydown", (event) => {
  if (event.key !== "Escape") return;
  closeChartPanel();
  canvas.focus();
});

canvas.addEventListener("pointerdown", (event) => {
  if (event.button !== 0) return;
  const { px, py } = pointerAt(event);
  lastPointer = { px, py };
  const grip = gripAt(px, py);
  if (grip) {
    resizing = grip;
    resized = false;
    canvas.setPointerCapture(event.pointerId);
    return;
  }
  // A seta da lista suspensa da célula ativa
  const arrow = listArrow();
  if (arrow && px >= arrow.left && px <= arrow.left + arrow.size && py >= arrow.top && py <= arrow.top + arrow.size) {
    event.preventDefault();
    void openListMenu();
    return;
  }
  // Um gráfico sob o mouse: seleciona e começa a mover, ou a redimensionar pela alça.
  const hit = px >= HEADER_WIDTH && py >= HEADER_HEIGHT ? chartAt(px, py) : null;
  if (hit) {
    selectedChart = hit.id;
    chartDrag = {
      id: hit.id,
      mode: onChartHandle(hit, px, py) ? "resize" : "move",
      grab: cellAt(px, py),
      origin: { ...hit },
      moved: false,
    };
    canvas.setPointerCapture(event.pointerId);
    if (!chartPanel.hidden) openChartPanel(hit);
    void write();
    draw();
    return;
  }
  if (selectedChart !== null) {
    selectedChart = null;
    closeChartPanel();
  }
  const handle = fillHandle();
  if (handle && Math.abs(px - handle.x) <= HANDLE_GRIP && Math.abs(py - handle.y) <= HANDLE_GRIP) {
    fillDrag = true;
    fillTarget = null;
    canvas.setPointerCapture(event.pointerId);
    return;
  }
  const cell = cellAt(px, py);
  canvas.setPointerCapture(event.pointerId);
  // Clicar num cabeçalho seleciona a coluna ou a linha inteira; no canto, tudo.
  const header = headerAt(px, py);
  if (header) {
    if (header !== "corner") headerDrag = { kind: header, from: cell };
    void write().then((written) => {
      if (written) selectHeaders(header, cell, cell);
      else headerDrag = null;
    });
    return;
  }
  // No meio de uma fórmula, clicar numa célula insere a referência a ela.
  const input = activeInput();
  if (input && canPoint(input)) {
    pointDrag = true;
    pointAt(input, cell, cell);
    return;
  }
  dragging = true;
  // Clicar em outra célula no meio de uma edição grava a edição, como no Excel.
  void (async () => {
    if (!(await write())) return;
    await select(cell, event.shiftKey);
  })();
});

canvas.addEventListener("pointermove", (event) => {
  const { px, py } = pointerAt(event);
  if (resizing) {
    // Mostra o novo tamanho enquanto arrasta; o motor só é avisado ao soltar.
    resized = true;
    if (resizing.kind === "col") {
      colWidths[resizing.index] = Math.max(MIN_SIZE, Math.round(px - colX[resizing.index]));
      colX = edges(colWidths, HEADER_WIDTH);
    } else {
      rowHeights[resizing.index] = Math.max(MIN_SIZE, Math.round(py - rowY[resizing.index]));
      rowY = edges(rowHeights, HEADER_HEIGHT);
    }
    draw();
    return;
  }
  if (chartDrag) {
    dragChart(px, py);
    return;
  }
  if (dragging || fillDrag || pointDrag || headerDrag) {
    lastPointer = { px, py };
    dragTo(px, py);
    autoScroll();
    return;
  }
  const grip = gripAt(px, py);
  const handle = fillHandle();
  const onHandle = handle !== null && Math.abs(px - handle.x) <= HANDLE_GRIP && Math.abs(py - handle.y) <= HANDLE_GRIP;
  const header = headerAt(px, py);
  const overChart = px >= HEADER_WIDTH && py >= HEADER_HEIGHT ? chartAt(px, py) : null;
  canvas.style.cursor = overChart
    ? onChartHandle(overChart, px, py)
      ? "nwse-resize"
      : "move"
    : grip
    ? grip.kind === "col"
      ? "col-resize"
      : "row-resize"
    : onHandle
      ? "crosshair"
      : header
        ? "pointer"
        : "cell";
});

const endDrag = () => {
  stopAutoScroll();
  if (chartDrag) {
    const drag = chartDrag;
    chartDrag = null;
    const chart = charts.find((one) => one.id === drag.id);
    if (chart && drag.moved) void saveChart(chart);
  }
  dragging = false;
  headerDrag = null;
  if (resizing) {
    const grip = resizing;
    resizing = null;
    // Um clique na borda, sem arrastar, não muda nada.
    if (resized) void resize(grip, grip.kind === "col" ? colWidths[grip.index] : rowHeights[grip.index]);
  }
  if (fillDrag) {
    fillDrag = false;
    const target = fillTarget;
    if (target) {
      void fillSelection(target).finally(() => {
        fillTarget = null;
        draw();
      });
    }
  }
  if (pointDrag) {
    pointDrag = false;
    // O clique tirou o foco do campo: devolve, com o cursor depois da referência.
    if (pointing) {
      pointing.input.focus();
      pointing.input.setSelectionRange(pointing.end, pointing.end);
    }
  }
};
canvas.addEventListener("pointerup", endDrag);
canvas.addEventListener("pointercancel", endDrag);

// Dois cliques na borda de uma coluna ajustam a largura ao conteúdo visível.
canvas.addEventListener("dblclick", (event) => {
  const { px, py } = pointerAt(event);
  const grip = gripAt(px, py);
  const chart = px >= HEADER_WIDTH && py >= HEADER_HEIGHT ? chartAt(px, py) : null;
  if (chart) {
    selectedChart = chart.id;
    openChartPanel(chart);
    draw();
    return;
  }
  // Dois cliques numa célula abrem a edição do conteúdo dela.
  if (!grip && px >= HEADER_WIDTH && py >= HEADER_HEIGHT && editMode === null) {
    startEdit(null, "edit");
    return;
  }
  if (grip?.kind !== "col") return;
  const col = bandCol(grip.index);
  let widest = 0;
  for (const cell of cells.values()) {
    if (cell.col !== col || cell.display === "") continue;
    ctx.font = cellFont(cell);
    widest = Math.max(widest, ctx.measureText(cell.display).width);
  }
  if (widest > 0) void resize(grip, Math.ceil(widest) + 2 * PADDING + 2);
});

canvas.addEventListener(
  "wheel",
  (event) => {
    event.preventDefault();
    const horizontal = event.shiftKey || Math.abs(event.deltaX) > Math.abs(event.deltaY);
    const delta = horizontal ? event.deltaX || event.deltaY : event.deltaY;
    if (delta === 0) return;
    const { origin } = pos;
    const dir = Math.sign(delta);
    if (horizontal) {
      let col = origin.col + dir;
      // Uma coluna oculta não ocupa espaço: rolar até ela não mudaria nada.
      while (col > frozen.cols && colOnScreen(col) && colWidths[colBand(col)] === 0) col += dir;
      origin.col = Math.max(frozen.cols, clamp(col, MAX_COLS - 1));
    } else {
      // Três linhas visíveis por vez.
      let row = origin.row;
      for (let steps = 0; steps < 3; steps++) {
        row += dir;
        while (row > frozen.rows && rowOnScreen(row) && rowHeights[rowBand(row)] === 0) row += dir;
      }
      origin.row = Math.max(frozen.rows, clamp(row, MAX_ROWS - 1));
    }
    void refresh();
  },
  { passive: false },
);

canvas.addEventListener("keydown", (event) => {
  const moves: Record<string, [number, number]> = {
    ArrowLeft: [-1, 0],
    ArrowRight: [1, 0],
    ArrowUp: [0, -1],
    ArrowDown: [0, 1],
  };
  const arrow = moves[event.key];
  const shortcut = event.ctrlKey || event.metaKey;
  const letter = event.key.toLowerCase();
  const toggles: Record<string, "bold" | "italic" | "underline"> = { b: "bold", i: "italic", u: "underline" };

  // Com um gráfico selecionado, Delete o apaga, Enter abre as opções e Esc solta a seleção.
  if (selectedChart !== null && !shortcut) {
    const chart = charts.find((one) => one.id === selectedChart);
    if (chart && (event.key === "Delete" || event.key === "Backspace")) {
      event.preventDefault();
      void removeChart(chart.id);
      return;
    }
    if (chart && event.key === "Enter") {
      event.preventDefault();
      openChartPanel(chart);
      return;
    }
    selectedChart = null;
    closeChartPanel();
    draw();
    if (event.key === "Escape") return;
  }

  if (shortcut && letter === "z") {
    event.preventDefault();
    void history(event.shiftKey ? "redo" : "undo");
  } else if (shortcut && letter === "y") {
    event.preventDefault();
    void history("redo");
  } else if (shortcut && (letter === "c" || letter === "x")) {
    event.preventDefault();
    void copySelection(letter === "x");
  } else if (shortcut && letter === "v") {
    event.preventDefault();
    void pasteAtSelection();
  } else if (event.key === "Escape") {
    // Desiste do copiar ou recortar em andamento (só a marca; o conteúdo
    // continua na área de transferência).
    copiedMark = null;
    draw();
  } else if (shortcut && letter === "a") {
    // Sem isto o navegador selecionaria o texto da página inteira.
    event.preventDefault();
  } else if (shortcut && toggles[letter] && engine.formatting) {
    event.preventDefault();
    void toggle(toggles[letter]);
  } else if (shortcut && (arrow || event.key === "Home" || event.key === "End")) {
    event.preventDefault();
    void jumpTo(event.key, event.shiftKey);
  } else if (event.key === "Home" || event.key === "PageUp" || event.key === "PageDown") {
    event.preventDefault();
    void pageTo(event.key, event.shiftKey);
  } else if (arrow) {
    event.preventDefault();
    // Com Shift a seleção cresce a partir do foco; sem ele, anda a célula ativa.
    if (event.shiftKey) void select({ col: pos.focus.col + arrow[0], row: pos.focus.row + arrow[1] }, true);
    else void move(arrow[0], arrow[1]);
  } else if (event.key === "Tab") {
    event.preventDefault();
    void move(event.shiftKey ? -1 : 1, 0);
  } else if (event.key === "Enter") {
    event.preventDefault();
    void move(0, event.shiftKey ? -1 : 1);
  } else if (event.key === "F2") {
    event.preventDefault();
    startEdit(null, "edit");
  } else if (event.key === "Delete" || event.key === "Backspace") {
    event.preventDefault();
    void clearSelection();
  } else if (event.key.length === 1 && !shortcut && !event.altKey) {
    // Digitar sobre a célula começa uma edição nova, como no Excel.
    event.preventDefault();
    startEdit(event.key, "enter");
  }
});

editor.addEventListener("input", () => {
  fx.value = editor.value;
  dirty = true;
  pointing = null;
  updateSuggestions(editor);
  draw();
});

editor.addEventListener("keydown", (event) => {
  const moves: Record<string, [number, number]> = {
    ArrowLeft: [-1, 0],
    ArrowRight: [1, 0],
    ArrowUp: [0, -1],
    ArrowDown: [0, 1],
  };
  const arrow = moves[event.key];
  if (suggestionKey(event)) return;
  if (event.key === "Enter") {
    event.preventDefault();
    void commit(0, event.shiftKey ? -1 : 1);
  } else if (event.key === "Tab") {
    event.preventDefault();
    void commit(event.shiftKey ? -1 : 1, 0);
  } else if (event.key === "Escape") {
    event.preventDefault();
    cancelEdit();
  } else if (arrow && editMode === "enter" && !editor.value.startsWith("=")) {
    // Quem começou digitando um valor grava e anda com as setas, como no
    // Excel. Em fórmulas as setas só movem o cursor.
    event.preventDefault();
    void commit(arrow[0], arrow[1]);
  } else {
    pointing = null;
  }
});

fx.addEventListener("input", () => {
  dirty = true;
  pointing = null;
  if (editMode !== null) editor.value = fx.value;
  updateSuggestions(fx);
  draw();
});

// Os intervalos da fórmula aparecem coloridos enquanto a barra tem o foco.
fx.addEventListener("focus", draw);
fx.addEventListener("blur", () => {
  if (suggestion?.input === fx) closeSuggestions();
  draw();
});
editor.addEventListener("blur", () => {
  if (suggestion?.input === editor) closeSuggestions();
});

fx.addEventListener("keydown", (event) => {
  if (suggestionKey(event)) return;
  if (event.key === "Enter") {
    event.preventDefault();
    void commit(0, event.shiftKey ? -1 : 1);
  } else if (event.key === "Tab") {
    event.preventDefault();
    void commit(event.shiftKey ? -1 : 1, 0);
  } else if (event.key === "Escape") {
    event.preventDefault();
    cancelEdit();
  } else {
    pointing = null;
  }
});

// Atalhos de arquivo valem na janela toda.
window.addEventListener("keydown", (event) => {
  if (!(event.ctrlKey || event.metaKey) || event.altKey) return;
  const letter = event.key.toLowerCase();
  if (letter === "o") {
    event.preventDefault();
    void openFile();
  } else if (letter === "s") {
    event.preventDefault();
    void saveFile(event.shiftKey);
  } else if (letter === "f" || letter === "h") {
    event.preventDefault();
    void write().then((written) => {
      if (!written) return;
      openFind();
      if (letter === "h") replaceText.focus();
    });
  }
});

findQuery.addEventListener("input", () => void runFind().then(() => findStep(true, true)));
for (const input of [findQuery, replaceText]) {
  input.addEventListener("keydown", (event) => {
    if (event.key === "Enter") {
      event.preventDefault();
      void findStep(!event.shiftKey);
    } else if (event.key === "Escape") {
      event.preventDefault();
      closeFind();
    }
  });
}
$("#find-btn").addEventListener("click", () => void write().then((written) => written && openFind()));
$("#replace-btn").addEventListener("click", () =>
  void write().then((written) => {
    if (!written) return;
    openFind();
    replaceText.focus();
  }),
);
$("#find-next").addEventListener("click", () => void findStep(true));
$("#find-prev").addEventListener("click", () => void findStep(false));
$("#find-close").addEventListener("click", closeFind);
$("#replace-one").addEventListener("click", () => void replaceCurrent());
$("#replace-all").addEventListener("click", () => void replaceEverything());

$("#filter-all").addEventListener("click", () => {
  const boxes = [...filterValues.querySelectorAll<HTMLInputElement>("input")];
  // Com tudo marcado, o botão desmarca tudo.
  const check = boxes.some((box) => !box.checked);
  for (const box of boxes) box.checked = check;
});
$("#filter-apply").addEventListener("click", () => void applyFilter());
$("#filter-cancel").addEventListener("click", closeFilter);
filterPanel.addEventListener("keydown", (event) => {
  if (event.key === "Escape") closeFilter();
  else if (event.key === "Enter") void applyFilter();
});

for (const button of document.querySelectorAll<HTMLElement>("[data-toggle]")) {
  button.addEventListener("click", () => void toggle(button.dataset.toggle as ToggleProperty));
}
for (const button of document.querySelectorAll<HTMLElement>("[data-align]")) {
  // Clicar no alinhamento já ativo volta ao automático.
  button.addEventListener("click", () => {
    const active = button.getAttribute("aria-pressed") === "true";
    void format("align", active ? "general" : button.dataset.align!);
  });
}
for (const button of document.querySelectorAll<HTMLElement>("[data-format]")) {
  button.addEventListener("click", () => void format("format", button.dataset.format!));
}
// `change` dispara uma vez, ao fechar o seletor; `input` dispararia a cada movimento.
textColor.addEventListener("change", () => void format("color", textColor.value));
fillColor.addEventListener("change", () => void format("fill", fillColor.value));
$("#no-fill").addEventListener("click", () => void format("fill", ""));

for (const button of document.querySelectorAll<HTMLElement>("[data-size]")) {
  button.addEventListener("click", () => {
    const cell = { ...pos.anchor };
    void format("size", button.dataset.size!).then(() => fitRow(cell));
  });
}
valignSelect.addEventListener("change", () => {
  const value = valignSelect.value;
  // Como a lista de bordas, esta é um menu de ações e volta ao rótulo.
  valignSelect.value = "";
  if (value !== "") void format("valign", value);
});
mergeButton.addEventListener("click", () => void toggleMerge());
borderSelect.addEventListener("change", () => {
  const kind = borderSelect.value as BorderKind | "";
  // A lista volta ao rótulo "Bordas": ela é um menu de ações, não um estado.
  borderSelect.value = "";
  if (kind !== "") void change("mudar as bordas", () => engine.setBorder(sheet, selectedRange(), kind));
});

// Menu do botão direito: age sobre a seleção, ou sobre a célula clicada se
// ela estiver fora da seleção.
canvas.addEventListener("contextmenu", (event) => {
  event.preventDefault();
  const { px, py } = pointerAt(event);
  const overChart = px >= HEADER_WIDTH && py >= HEADER_HEIGHT ? chartAt(px, py) : null;
  if (overChart) {
    selectedChart = overChart.id;
    openChartPanel(overChart);
    draw();
    return;
  }
  const cell = cellAt(px, py);
  const range = selectedRange();
  const header = headerAt(px, py);
  void (async () => {
    if (!(await write())) return;
    if (header === "col" || header === "row") {
      // Num cabeçalho, o menu vale para a coluna ou linha inteira.
      const whole = header === "col" ? range.rows === MAX_ROWS : range.cols === MAX_COLS;
      const inside = header === "col" ? cell.col >= range.col && cell.col < range.col + range.cols : cell.row >= range.row && cell.row < range.row + range.rows;
      if (!whole || !inside) selectHeaders(header, cell, cell);
    } else if (!contains(range, cell.col, cell.row)) {
      await select(cell);
    }
    // Num cabeçalho só aparecem as ações daquele eixo.
    for (const item of menu.querySelectorAll<HTMLElement>("[data-scope]")) {
      const scope = item.dataset.scope;
      item.hidden = (header === "col" && scope === "row") || (header === "row" && scope !== "row");
    }
    for (const item of menu.querySelectorAll<HTMLElement>("[data-tables]")) {
      if (!engine.tables) item.hidden = true;
    }
    menuAt = { left: event.clientX, top: event.clientY };
    menu.hidden = false;
    menu.style.left = `${Math.max(0, Math.min(event.clientX, window.innerWidth - menu.offsetWidth - 8))}px`;
    menu.style.top = `${Math.max(0, Math.min(event.clientY, window.innerHeight - menu.offsetHeight - 8))}px`;
  })();
});

/** Onde o menu abriu, para o painel de filtro abrir no mesmo lugar. */
let menuAt = { left: 0, top: 0 };
const closeMenu = () => {
  menu.hidden = true;
};
document.addEventListener("pointerdown", (event) => {
  if (!menu.hidden && !menu.contains(event.target as Node)) closeMenu();
  if (!dvMenu.hidden && !dvMenu.contains(event.target as Node)) closeListMenu();
  if (!(event.target as HTMLElement).closest?.("#pivot-menu, .pvt-field")) pivotPanel.closeMenu();
  if (!filterPanel.hidden && !filterPanel.contains(event.target as Node)) closeFilter();
});
window.addEventListener("blur", closeMenu);
window.addEventListener("keydown", (event) => {
  if (event.key === "Escape") {
    closeMenu();
    closeListMenu();
  }
});

/** Executa uma ação do menu de contexto ou da faixa de opções; `at` é onde abrir o painel de filtro. */
function runAction(action: string, at: { left: number; top: number }) {
  if (action === "cut" || action === "copy") void copySelection(action === "cut");
  else if (action === "paste") void pasteAtSelection();
  else if (action === "sort-asc" || action === "sort-desc") void sortSelection(action === "sort-desc");
  else if (action === "filter") void openFilter(at.left, at.top);
  else if (action === "clear-filter") void clearFilter();
  else if (action === "hide-rows" || action === "show-rows") void setHidden("rows", action === "hide-rows");
  else if (action === "hide-cols" || action === "show-cols") void setHidden("cols", action === "hide-cols");
  else if (action.startsWith("chart-")) void insertChart(action.slice(6) as ChartKind);
  else if (action === "cond-open") void openConditional();
  else if (action === "dv-open") void openValidation();
  else if (action === "pivot-new") void pivotPanel.create();
  else if (action === "pivot-refresh") void refreshPivots();
  else if (action === "freeze") void freezeAtSelection();
  else if (action === "unfreeze") void change("descongelar", () => engine.setFrozen(sheet, 0, 0));
  else void changeStructure(action);
}

menu.addEventListener("click", (event) => {
  const action = (event.target as HTMLElement).closest<HTMLElement>("[data-action]")?.dataset.action;
  if (!action) return;
  closeMenu();
  canvas.focus();
  runAction(action, menuAt);
});

// Faixa de opções: os botões agem sobre a seleção, como o menu de contexto.
$("#toolbar").addEventListener("click", (event) => {
  const button = (event.target as HTMLElement).closest<HTMLElement>("[data-action]");
  if (!button?.dataset.action) return;
  const box = button.getBoundingClientRect();
  canvas.focus();
  runAction(button.dataset.action, { left: box.left, top: box.bottom + 4 });
});

/** Mostra o painel da aba escolhida e esconde os demais. */
function selectTab(tab: HTMLElement, focus = false) {
  for (const other of document.querySelectorAll<HTMLElement>("[data-tab]")) {
    const selected = other === tab;
    other.setAttribute("aria-selected", String(selected));
    other.tabIndex = selected ? 0 : -1;
    $(`#panel-${other.dataset.tab}`).hidden = !selected;
  }
  if (focus) tab.focus();
}
const tabs = [...document.querySelectorAll<HTMLElement>("[data-tab]")];
for (const tab of tabs) {
  tab.addEventListener("click", () => selectTab(tab));
  tab.addEventListener("keydown", (event) => {
    const shift = event.key === "ArrowRight" ? 1 : event.key === "ArrowLeft" ? -1 : 0;
    if (shift === 0) return;
    event.preventDefault();
    selectTab(tabs[(tabs.indexOf(tab) + shift + tabs.length) % tabs.length], true);
  });
}

/** Refaz todas as tabelas dinâmicas a partir dos dados de origem. */
async function refreshPivots() {
  if (!(await write())) return;
  try {
    const count = await engine.refreshAllPivots();
    statusEl.textContent = count === 0 ? "Não há tabelas dinâmicas" : "Tabelas dinâmicas atualizadas";
    if (count > 0) markModified();
    await refresh();
  } catch (error) {
    statusEl.textContent = `Erro ao atualizar as tabelas dinâmicas: ${error}`;
  }
}

// Painel da tabela dinâmica.
const pivotPanel = setupPivotPanel({
  engine,
  sheet: () => sheet,
  sourceText: async () => {
    const range = selectedRange();
    const picked = singleSelection() ? await engine.dataRegion(sheet, pos.anchor.col, pos.anchor.row) : range;
    return rangeText(picked);
  },
  addSheet: async () => {
    const index = await engine.addSheet();
    await loadSheets();
    await showSheet(index);
    return index;
  },
  changed: async () => {
    markModified();
    await refresh();
  },
  status: (text) => {
    statusEl.textContent = text;
  },
});

// Painel de validação de dados.
const openValidation = setupValidationPanel({
  engine,
  sheet: () => sheet,
  selection: () => rangeText(selectedRange()),
  changed: async () => {
    markModified();
    await refresh();
  },
});

// Painel de formatação condicional.
const openConditional = setupConditionalPanel({
  engine,
  sheet: () => sheet,
  selection: () => rangeText(selectedRange()),
  changed: async () => {
    markModified();
    await refresh();
  },
});

// Fechar a janela com alterações não salvas pergunta antes. Uma edição em
// andamento entra no documento primeiro; se ela for inválida, as alterações
// anteriores ainda são oferecidas para salvar.
engine.onClose(async () => {
  await write();
  return settleChanges();
});

$("#open-btn").addEventListener("click", () => void openFile());
$("#save-btn").addEventListener("click", () => void saveFile(false));
$("#save-as-btn").addEventListener("click", () => void saveFile(true));
$("#add-sheet").addEventListener("click", () => void addSheet());
$("#file-actions").hidden = engine.files === null;
$("#toolbar").hidden = !engine.formatting;

new ResizeObserver(() => void refresh()).observe(wrap);

syncDocument();
canvas.focus();
void loadSheets().then(refresh);
