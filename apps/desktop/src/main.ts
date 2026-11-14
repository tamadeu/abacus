import { createEngine, type CellView, type Range, type SheetInfo, type StyleProperty } from "./engine";

const HEADER_WIDTH = 48;
const HEADER_HEIGHT = 26;
// Limites de uma planilha, iguais aos do Excel.
const MAX_COLS = 16_384;
const MAX_ROWS = 1_048_576;
const FONT = "Inter, system-ui, sans-serif";
/** Espaço entre o texto e a borda da célula. */
const PADDING = 8;

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

const engine = createEngine();
const $ = <T extends HTMLElement>(selector: string) => document.querySelector<T>(selector)!;
const wrap = $("#grid-wrap");
const canvas = $<HTMLCanvasElement>("#grid");
const ctx = canvas.getContext("2d")!;
const nameBox = $("#name-box");
const fx = $<HTMLInputElement>("#fx");
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

/** Retângulo selecionado, da âncora ao foco. */
function selectedRange(): Range {
  const { anchor, focus } = pos;
  return {
    col: Math.min(anchor.col, focus.col),
    row: Math.min(anchor.row, focus.row),
    cols: Math.abs(anchor.col - focus.col) + 1,
    rows: Math.abs(anchor.row - focus.row) + 1,
  };
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

  // Fundos coloridos, antes do texto. Como no Excel, cobrem as linhas de
  // grade à direita e abaixo da célula.
  for (const cell of cells.values()) {
    if (!cell.style?.fill) continue;
    ctx.fillStyle = cell.style.fill;
    ctx.fillRect(xOf(cell.col) + 1, yOf(cell.row) + 1, xOf(cell.col + 1) - xOf(cell.col), yOf(cell.row + 1) - yOf(cell.row));
  }

  // Conteúdo das células
  ctx.textBaseline = "middle";
  for (const cell of cells.values()) {
    if (cell.display === "") continue;
    const style = cell.style;
    const left = xOf(cell.col);
    const top = yOf(cell.row);
    const cellWidth = xOf(cell.col + 1) - left;
    const cellHeight = yOf(cell.row + 1) - top;
    const middle = top + cellHeight / 2 + 1;
    const align = style?.align ?? (cell.kind === "number" ? "right" : cell.kind === "error" ? "center" : "left");
    const anchorX = align === "right" ? left + cellWidth - PADDING : align === "center" ? left + cellWidth / 2 : left + PADDING;

    ctx.save();
    ctx.font = `${style?.italic ? "italic " : ""}${style?.bold ? "700 " : ""}14px ${FONT}`;
    // Como no Excel, texto alinhado à esquerda invade as células vazias à direita.
    let clipWidth = cellWidth;
    if (cell.kind === "text" && align === "left") {
      const needed = ctx.measureText(cell.display).width + 2 * PADDING;
      const lastCol = origin.col + colWidths.length - 1;
      for (let next = cell.col + 1; needed > clipWidth && next <= lastCol; next++) {
        if (cells.get(key(next, cell.row))?.display) break;
        clipWidth = xOf(next + 1) - left;
      }
    }
    ctx.beginPath();
    ctx.rect(left + 1, top + 1, clipWidth - 1, cellHeight - 1);
    ctx.clip();
    ctx.fillStyle = style?.color ?? (cell.kind === "error" ? COLORS.error : COLORS.ink);
    ctx.textAlign = align;
    ctx.fillText(cell.display, anchorX, middle);
    if (style?.underline || style?.strike) {
      const textWidth = ctx.measureText(cell.display).width;
      const start = align === "right" ? anchorX - textWidth : align === "center" ? anchorX - textWidth / 2 : anchorX;
      if (style.underline) ctx.fillRect(start, middle + 8, textWidth, 1);
      if (style.strike) ctx.fillRect(start, middle, textWidth, 1);
    }
    ctx.restore();
  }

  // Seleção: o intervalo fica tingido, menos a célula ativa.
  const selLeft = xOf(range.col);
  const selTop = yOf(range.row);
  const selWidth = xOf(range.col + range.cols) - selLeft;
  const selHeight = yOf(range.row + range.rows) - selTop;
  if (selWidth > 0 && selHeight > 0) {
    if (range.cols > 1 || range.rows > 1) {
      ctx.fillStyle = COLORS.selection;
      ctx.beginPath();
      ctx.rect(selLeft, selTop, selWidth, selHeight);
      ctx.rect(xOf(anchor.col), yOf(anchor.row), xOf(anchor.col + 1) - xOf(anchor.col), yOf(anchor.row + 1) - yOf(anchor.row));
      ctx.fill("evenodd");
    }
    ctx.strokeStyle = COLORS.accent;
    ctx.lineWidth = 2;
    ctx.strokeRect(selLeft + 0.5, selTop + 0.5, selWidth, selHeight);
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
}

/** Mostra a seleção na caixa de nome e a célula ativa na barra de fórmulas. */
function syncFormulaBar() {
  const { anchor, focus } = pos;
  const single = anchor.col === focus.col && anchor.row === focus.row;
  nameBox.textContent = single ? cellName(anchor) : `${cellName(anchor)}:${cellName(focus)}`;
  if (!dirty) fx.value = cells.get(key(anchor.col, anchor.row))?.input ?? "";
}

/** Faz a barra de ferramentas refletir o estilo da célula ativa. */
function syncToolbar() {
  const style = cells.get(key(pos.anchor.col, pos.anchor.row))?.style;
  for (const button of document.querySelectorAll<HTMLElement>("[data-toggle]")) {
    const property = button.dataset.toggle as "bold" | "italic" | "underline" | "strike";
    button.setAttribute("aria-pressed", String(style?.[property] ?? false));
  }
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

/** Move o foco da seleção; sem `extend`, a seleção vira essa única célula. */
async function select(cell: Cell, extend = false) {
  pos.focus = { col: clamp(cell.col, MAX_COLS - 1), row: clamp(cell.row, MAX_ROWS - 1) };
  if (!extend) pos.anchor = { ...pos.focus };
  await reveal();
}

/** Grava o texto da barra de fórmulas na célula ativa. */
async function write(): Promise<boolean> {
  const { anchor } = pos;
  const current = cells.get(key(anchor.col, anchor.row))?.input ?? "";
  if (!dirty || fx.value === current) {
    dirty = false;
    return true;
  }
  try {
    await engine.setCell(sheet, anchor.col, anchor.row, fx.value);
    dirty = false;
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
  if (!(await write())) return;
  canvas.focus();
  await select({ col: pos.anchor.col + moveCols, row: pos.anchor.row + moveRows });
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

/** Liga ou desliga negrito, itálico, sublinhado ou tachado, a partir da célula ativa. */
function toggle(property: "bold" | "italic" | "underline" | "strike") {
  const current = cells.get(key(pos.anchor.col, pos.anchor.row))?.style?.[property] ?? false;
  return format(property, String(!current));
}

/** Apaga o conteúdo da seleção. */
async function clearSelection() {
  dirty = false;
  try {
    await engine.clearCells(sheet, selectedRange());
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

/** Redesenha as abas de planilha. */
function renderTabs() {
  sheetTabs.replaceChildren(
    ...sheets.map((info) => {
      const tab = document.createElement("button");
      tab.type = "button";
      tab.setAttribute("role", "tab");
      tab.setAttribute("aria-selected", String(info.index === sheet));
      tab.textContent = info.name;
      tab.addEventListener("click", () => void showSheet(info.index));
      return tab;
    }),
  );
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
    if (modified && !(await files.confirmDiscard())) return;
    const path = await files.open();
    if (path === null) return;
    filePath = path;
    modified = false;
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
function pointerCell(event: PointerEvent): Cell {
  const bounds = canvas.getBoundingClientRect();
  return cellAt(event.clientX - bounds.left, event.clientY - bounds.top);
}

canvas.addEventListener("pointerdown", (event) => {
  if (event.button !== 0) return;
  const bounds = canvas.getBoundingClientRect();
  if (event.clientX - bounds.left < HEADER_WIDTH || event.clientY - bounds.top < HEADER_HEIGHT) return;
  const cell = pointerCell(event);
  dragging = true;
  canvas.setPointerCapture(event.pointerId);
  // Clicar em outra célula no meio de uma edição grava a edição, como no Excel.
  void (async () => {
    if (!(await write())) return;
    await select(cell, event.shiftKey);
  })();
});

canvas.addEventListener("pointermove", (event) => {
  if (!dragging) return;
  const cell = pointerCell(event);
  if (cell.col === pos.focus.col && cell.row === pos.focus.row) return;
  pos.focus = cell;
  draw();
  syncFormulaBar();
});

const endDrag = () => {
  dragging = false;
};
canvas.addEventListener("pointerup", endDrag);
canvas.addEventListener("pointercancel", endDrag);

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
  } else if (shortcut && toggles[letter] && engine.formatting) {
    event.preventDefault();
    void toggle(toggles[letter]);
  } else if (move) {
    event.preventDefault();
    // Com Shift a seleção cresce a partir do foco; sem ele, anda a célula ativa.
    const from = event.shiftKey ? pos.focus : pos.anchor;
    void select({ col: from.col + move[0], row: from.row + move[1] }, event.shiftKey);
  } else if (event.key === "Tab") {
    event.preventDefault();
    void select({ col: pos.anchor.col + (event.shiftKey ? -1 : 1), row: pos.anchor.row });
  } else if (event.key === "Enter" || event.key === "F2") {
    event.preventDefault();
    fx.focus();
  } else if (event.key === "Delete" || event.key === "Backspace") {
    event.preventDefault();
    void clearSelection();
  } else if (event.key.length === 1 && !shortcut && !event.altKey) {
    // Digitar sobre a célula começa uma edição nova, como no Excel.
    event.preventDefault();
    fx.value = event.key;
    dirty = true;
    fx.focus();
  }
});

fx.addEventListener("input", () => {
  dirty = true;
});

fx.addEventListener("keydown", (event) => {
  if (event.key === "Enter") {
    event.preventDefault();
    void commit(0, event.shiftKey ? -1 : 1);
  } else if (event.key === "Tab") {
    event.preventDefault();
    void commit(event.shiftKey ? -1 : 1, 0);
  } else if (event.key === "Escape") {
    event.preventDefault();
    dirty = false;
    syncFormulaBar();
    canvas.focus();
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
  button.addEventListener("click", () => void toggle(button.dataset.toggle as "bold" | "italic" | "underline" | "strike"));
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
