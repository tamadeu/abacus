import {
  createEngine,
  type BorderKind,
  type CellView,
  type Range,
  type SheetInfo,
  type Structure,
  type StyleProperty,
} from "./engine";
import { formulaRefs } from "./refs";

const HEADER_WIDTH = 48;
const HEADER_HEIGHT = 26;
// Limites de uma planilha, iguais aos do Excel.
const MAX_COLS = 16_384;
const MAX_ROWS = 1_048_576;
const FONT = "Inter, system-ui, sans-serif";
/** Espaço entre o texto e a borda da célula. */
const PADDING = 8;
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

/** Estilos que a barra de ferramentas liga e desliga. */
type ToggleProperty = "bold" | "italic" | "underline" | "strike" | "wrap";
const statusEl = $("#status");
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
/** Tamanhos das colunas e linhas da tela, a partir de `pos.origin`. */
let colWidths: number[] = [];
let rowHeights: number[] = [];
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
/** O mouse está arrastando para apontar um intervalo na fórmula. */
let pointDrag = false;
/** Destino do preenchimento enquanto a alça é arrastada; `null` se ainda não saiu da seleção. */
let fillTarget: { axis: "rows" | "cols"; to: number } | null = null;
/** A alça de preenchimento está sendo arrastada. */
let fillDrag = false;
/** Intervalo copiado ou recortado, que fica marcado com borda tracejada. */
let copiedMark: { sheet: number; range: Range; cut: boolean } | null = null;
/** Arquivo aberto, ou `null` para uma pasta nova. */
let filePath: string | null = null;
/** A pasta tem alterações que não estão no arquivo. */
let modified = false;

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

/** Célula a um passo da ativa, pulando por cima da área mesclada em que ela está. */
function step(dx: number, dy: number): Cell {
  const block = blockOf(pos.anchor);
  return {
    col: dx > 0 ? block.col + block.cols : dx < 0 ? block.col - 1 : pos.anchor.col,
    row: dy > 0 ? block.row + block.rows : dy < 0 ? block.row - 1 : pos.anchor.row,
  };
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

/** Pixel onde começa a coluna, limitado à área visível. */
const xOf = (col: number) => colX[clamp(col - pos.origin.col, colX.length - 1)];
/** Pixel onde começa a linha, limitado à área visível. */
const yOf = (row: number) => rowY[clamp(row - pos.origin.row, rowY.length - 1)];

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

/** Fonte usada para desenhar o texto de uma célula. */
const cellFont = (cell: CellView) => `${cell.style?.italic ? "italic " : ""}${cell.style?.bold ? "700 " : ""}14px ${FONT}`;

/** Retângulo de um intervalo na tela, limitado à área visível. */
function rectOf(range: Range) {
  const left = xOf(range.col);
  const top = yOf(range.row);
  return { left, top, width: xOf(range.col + range.cols) - left, height: yOf(range.row + range.rows) - top };
}

/** Centro da alça de preenchimento, ou `null` se o canto da seleção não está na tela. */
function fillHandle(): { x: number; y: number } | null {
  if (editMode !== null) return null;
  const range = selectedRange();
  const colIndex = range.col + range.cols - pos.origin.col;
  const rowIndex = range.row + range.rows - pos.origin.row;
  if (colIndex < 1 || colIndex >= colX.length || rowIndex < 1 || rowIndex >= rowY.length) return null;
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
    col: clamp(pos.origin.col + bandAt(colX, px), MAX_COLS - 1),
    row: clamp(pos.origin.row + bandAt(rowY, py), MAX_ROWS - 1),
  };
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
      const lastCol = origin.col + colWidths.length - 1;
      for (let next = cell.col + 1; needed > clipWidth && next <= lastCol; next++) {
        if (cells.get(key(next, cell.row))?.display || mergeAt({ col: next, row: cell.row })) break;
        clipWidth = xOf(next + 1) - left;
      }
    }
    ctx.beginPath();
    ctx.rect(left + 1, top + 1, clipWidth - 1, cellHeight - 1);
    ctx.clip();
    ctx.fillStyle = style?.color ?? (cell.kind === "error" ? COLORS.error : COLORS.ink);
    ctx.textAlign = align;
    // O bloco de linhas fica centrado na altura; se não couber, começa do topo.
    const blockTop = Math.max(top + 3, top + (cellHeight - lines.length * LINE_HEIGHT) / 2);
    lines.forEach((line, index) => {
      const middle = blockTop + index * LINE_HEIGHT + LINE_HEIGHT / 2 + 1;
      ctx.fillText(line, anchorX, middle);
      if (style?.underline || style?.strike) {
        const lineWidth = ctx.measureText(line).width;
        const start = align === "right" ? anchorX - lineWidth : align === "center" ? anchorX - lineWidth / 2 : anchorX;
        if (style.underline) ctx.fillRect(start, middle + 8, lineWidth, 1);
        if (style.strike) ctx.fillRect(start, middle, lineWidth, 1);
      }
    });
    ctx.restore();
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
    const col = origin.col + index;
    ctx.fillStyle = inRange(col, range.col, range.cols) ? COLORS.headerActiveInk : COLORS.muted;
    ctx.fillText(colName(col), colX[index] + colWidth / 2, HEADER_HEIGHT / 2);
  });
  rowHeights.forEach((rowHeight, index) => {
    const row = origin.row + index;
    ctx.fillStyle = inRange(row, range.row, range.rows) ? COLORS.headerActiveInk : COLORS.muted;
    ctx.fillText(String(row + 1), HEADER_WIDTH / 2, rowY[index] + rowHeight / 2);
  });

  positionEditor();
}

/** Põe o editor sobre a célula ativa, largo o bastante para o texto. */
function positionEditor() {
  if (editMode === null) return;
  const colIndex = pos.anchor.col - pos.origin.col;
  const rowIndex = pos.anchor.row - pos.origin.row;
  if (colIndex < 0 || colIndex >= colWidths.length || rowIndex < 0 || rowIndex >= rowHeights.length) {
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
  draw();
}

/** Mostra a seleção na caixa de nome e a célula ativa na barra de fórmulas. */
function syncFormulaBar() {
  const { anchor } = pos;
  const range = selectedRange();
  const last = { col: range.col + range.cols - 1, row: range.row + range.rows - 1 };
  nameBox.textContent = singleSelection() ? cellName(anchor) : `${cellName(range)}:${cellName(last)}`;
  if (!dirty) fx.value = cells.get(key(anchor.col, anchor.row))?.input ?? "";
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
}

/** Busca no motor o que cabe na tela e redesenha. */
async function refresh() {
  try {
    const width = Math.max(1, wrap.clientWidth - HEADER_WIDTH);
    const height = Math.max(1, wrap.clientHeight - HEADER_HEIGHT);
    const view = await engine.view(sheet, pos.origin.col, pos.origin.row, width, height);
    colWidths = view.colWidths;
    rowHeights = view.rowHeights;
    colX = edges(colWidths, HEADER_WIDTH);
    rowY = edges(rowHeights, HEADER_HEIGHT);
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
  origin.col = Math.min(origin.col, focus.col);
  origin.row = Math.min(origin.row, focus.row);
  await refresh();
  // As larguras variam, então avança uma coluna ou linha por vez e confere.
  for (let guard = 0; guard < 1000; guard++) {
    const colIndex = focus.col - origin.col;
    const rowIndex = focus.row - origin.row;
    const colCut = colIndex > 0 && (colIndex >= colWidths.length || colX[colIndex + 1] > wrap.clientWidth);
    const rowCut = rowIndex > 0 && (rowIndex >= rowHeights.length || rowY[rowIndex + 1] > wrap.clientHeight);
    if (!colCut && !rowCut) break;
    if (colCut) origin.col++;
    if (rowCut) origin.row++;
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
    statusEl.textContent = `Erro ao gravar a célula: ${error}`;
    return false;
  }
}

/** Grava a edição e move a célula ativa. */
async function commit(moveCols: number, moveRows: number) {
  const edited = { ...pos.anchor };
  if (!(await write())) return;
  canvas.focus();
  await select(step(moveCols, moveRows));
  await fitRow(edited);
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
 * Aumenta a altura da linha se o texto com quebra de uma célula não couber,
 * como o Excel faz. Nunca diminui: a altura pode ter sido escolhida à mão.
 */
async function fitRow(cell: Cell) {
  const view = cells.get(key(cell.col, cell.row));
  const rowIndex = cell.row - pos.origin.row;
  if (!view?.style?.wrap || view.kind !== "text" || mergeAt(cell) || rowIndex < 0 || rowIndex >= rowHeights.length) return;
  ctx.font = cellFont(view);
  const width = xOf(cell.col + 1) - xOf(cell.col) - 2 * PADDING;
  const needed = wrapLines(view.display, width).length * LINE_HEIGHT + 8;
  if (needed <= rowHeights[rowIndex]) return;
  try {
    await engine.setRowHeight(sheet, cell.row, Math.min(needed, 2000));
  } catch (error) {
    statusEl.textContent = `Erro ao ajustar a altura da linha: ${error}`;
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

async function openFile() {
  const files = engine.files;
  if (!files) return;
  if (!(await write())) return;
  try {
    if (modified && !(await engine.confirm("As alterações não salvas serão perdidas. Continuar?", "Descartar alterações"))) {
      return;
    }
    const path = await files.open();
    if (path === null) return;
    filePath = path;
    modified = false;
    copiedMark = null;
    positions = new Map();
    sheet = 0;
    pos = newPosition();
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
    const path = await files.save(askWhere ? null : filePath);
    if (path === null) return;
    filePath = path;
    modified = false;
    statusEl.textContent = "Arquivo salvo";
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
    if (grip.kind === "col") await engine.setColWidth(sheet, pos.origin.col + grip.index, size);
    else await engine.setRowHeight(sheet, pos.origin.row + grip.index, size);
    statusEl.textContent = "Pronto";
    markModified();
  } catch (error) {
    statusEl.textContent = `Erro ao redimensionar: ${error}`;
  }
  await refresh();
}

canvas.addEventListener("pointerdown", (event) => {
  if (event.button !== 0) return;
  const { px, py } = pointerAt(event);
  const grip = gripAt(px, py);
  if (grip) {
    resizing = grip;
    resized = false;
    canvas.setPointerCapture(event.pointerId);
    return;
  }
  const handle = fillHandle();
  if (handle && Math.abs(px - handle.x) <= HANDLE_GRIP && Math.abs(py - handle.y) <= HANDLE_GRIP) {
    fillDrag = true;
    fillTarget = null;
    canvas.setPointerCapture(event.pointerId);
    return;
  }
  if (px < HEADER_WIDTH || py < HEADER_HEIGHT) return;
  const cell = cellAt(px, py);
  canvas.setPointerCapture(event.pointerId);
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
  if (fillDrag) {
    // Preenche no sentido em que o mouse mais se afastou da seleção.
    const cell = cellAt(px, py);
    const range = selectedRange();
    const outside = (value: number, start: number, size: number) =>
      value < start ? value - start : value >= start + size ? value - (start + size - 1) : 0;
    const rows = outside(cell.row, range.row, range.rows);
    const cols = outside(cell.col, range.col, range.cols);
    if (rows === 0 && cols === 0) fillTarget = null;
    else if (Math.abs(rows) >= Math.abs(cols)) fillTarget = { axis: "rows", to: cell.row };
    else fillTarget = { axis: "cols", to: cell.col };
    draw();
    return;
  }
  if (pointDrag) {
    if (pointing) pointAt(pointing.input, pointing.from, cellAt(px, py));
    return;
  }
  if (!dragging) {
    const grip = gripAt(px, py);
    const handle = fillHandle();
    const onHandle = handle !== null && Math.abs(px - handle.x) <= HANDLE_GRIP && Math.abs(py - handle.y) <= HANDLE_GRIP;
    canvas.style.cursor = grip ? (grip.kind === "col" ? "col-resize" : "row-resize") : onHandle ? "crosshair" : "cell";
    return;
  }
  const cell = cellAt(px, py);
  if (cell.col === pos.focus.col && cell.row === pos.focus.row) return;
  pos.focus = cell;
  draw();
  syncFormulaBar();
});

const endDrag = () => {
  dragging = false;
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
  // Dois cliques numa célula abrem a edição do conteúdo dela.
  if (!grip && px >= HEADER_WIDTH && py >= HEADER_HEIGHT && editMode === null) {
    startEdit(null, "edit");
    return;
  }
  if (grip?.kind !== "col") return;
  const col = pos.origin.col + grip.index;
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
    if (horizontal) {
      origin.col = clamp(origin.col + Math.sign(delta), MAX_COLS - 1);
    } else {
      origin.row = clamp(origin.row + Math.sign(delta) * 3, MAX_ROWS - 1);
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
  const move = moves[event.key];
  const shortcut = event.ctrlKey || event.metaKey;
  const letter = event.key.toLowerCase();
  const toggles: Record<string, "bold" | "italic" | "underline"> = { b: "bold", i: "italic", u: "underline" };

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
  } else if (move) {
    event.preventDefault();
    // Com Shift a seleção cresce a partir do foco; sem ele, anda a célula ativa.
    const target = event.shiftKey ? { col: pos.focus.col + move[0], row: pos.focus.row + move[1] } : step(move[0], move[1]);
    void select(target, event.shiftKey);
  } else if (event.key === "Tab") {
    event.preventDefault();
    void select(step(event.shiftKey ? -1 : 1, 0));
  } else if (event.key === "Enter") {
    event.preventDefault();
    void select(step(0, event.shiftKey ? -1 : 1));
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
  draw();
});

editor.addEventListener("keydown", (event) => {
  const moves: Record<string, [number, number]> = {
    ArrowLeft: [-1, 0],
    ArrowRight: [1, 0],
    ArrowUp: [0, -1],
    ArrowDown: [0, 1],
  };
  const move = moves[event.key];
  if (event.key === "Enter") {
    event.preventDefault();
    void commit(0, event.shiftKey ? -1 : 1);
  } else if (event.key === "Tab") {
    event.preventDefault();
    void commit(event.shiftKey ? -1 : 1, 0);
  } else if (event.key === "Escape") {
    event.preventDefault();
    cancelEdit();
  } else if (move && editMode === "enter" && !editor.value.startsWith("=")) {
    // Quem começou digitando um valor grava e anda com as setas, como no
    // Excel. Em fórmulas as setas só movem o cursor.
    event.preventDefault();
    void commit(move[0], move[1]);
  } else {
    pointing = null;
  }
});

fx.addEventListener("input", () => {
  dirty = true;
  pointing = null;
  if (editMode !== null) editor.value = fx.value;
  draw();
});

// Os intervalos da fórmula aparecem coloridos enquanto a barra tem o foco.
fx.addEventListener("focus", draw);
fx.addEventListener("blur", draw);

fx.addEventListener("keydown", (event) => {
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
  }
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
  const cell = cellAt(px, py);
  const range = selectedRange();
  void (async () => {
    if (!(await write())) return;
    if (!contains(range, cell.col, cell.row)) await select(cell);
    menu.hidden = false;
    menu.style.left = `${Math.max(0, Math.min(event.clientX, window.innerWidth - menu.offsetWidth - 8))}px`;
    menu.style.top = `${Math.max(0, Math.min(event.clientY, window.innerHeight - menu.offsetHeight - 8))}px`;
  })();
});

const closeMenu = () => {
  menu.hidden = true;
};
document.addEventListener("pointerdown", (event) => {
  if (!menu.hidden && !menu.contains(event.target as Node)) closeMenu();
});
window.addEventListener("blur", closeMenu);
window.addEventListener("keydown", (event) => {
  if (event.key === "Escape") closeMenu();
});

menu.addEventListener("click", (event) => {
  const action = (event.target as HTMLElement).closest<HTMLElement>("[data-action]")?.dataset.action;
  if (!action) return;
  closeMenu();
  canvas.focus();
  if (action === "cut" || action === "copy") void copySelection(action === "cut");
  else if (action === "paste") void pasteAtSelection();
  else void changeStructure(action);
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
