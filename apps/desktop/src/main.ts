import { createEngine, type CellView, type SheetInfo } from "./engine";

const COL_WIDTH = 110;
const ROW_HEIGHT = 26;
const HEADER_WIDTH = 48;
const HEADER_HEIGHT = 26;
// Limites de uma planilha, iguais aos do Excel.
const MAX_COLS = 16_384;
const MAX_ROWS = 1_048_576;
const FONT = "Inter, system-ui, sans-serif";

const COLORS = {
  ink: "#1b1b29",
  muted: "#5b5b6b",
  line: "#e4e4ec",
  header: "#f6f6fa",
  headerActive: "#dcd9fa",
  headerActiveInk: "#3730a3",
  accent: "#4f46e5",
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

/** Onde a pessoa está em uma planilha. */
interface Position {
  /** Célula selecionada. */
  selection: { col: number; row: number };
  /** Primeira coluna e linha visíveis (a rolagem é por célula inteira). */
  origin: { col: number; row: number };
}

const newPosition = (): Position => ({ selection: { col: 0, row: 0 }, origin: { col: 0, row: 0 } });

/** Planilhas da pasta aberta e a posição guardada em cada uma. */
let sheets: SheetInfo[] = [];
let positions = new Map<number, Position>();
/** Índice da planilha na tela. */
let sheet = 0;
let { selection, origin } = newPosition();
/** Células da janela visível, por "coluna,linha". */
let cells = new Map<string, CellView>();
/** Há texto na barra de fórmulas que ainda não foi gravado na célula. */
let dirty = false;
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

/** Quantas colunas e linhas cabem (mesmo que parcialmente) na tela. */
function visibleSize() {
  return {
    cols: Math.max(1, Math.ceil((wrap.clientWidth - HEADER_WIDTH) / COL_WIDTH)),
    rows: Math.max(1, Math.ceil((wrap.clientHeight - HEADER_HEIGHT) / ROW_HEIGHT)),
  };
}

/** Ajusta a rolagem para a célula selecionada ficar inteira na tela. */
function scrollToSelection() {
  const fullCols = Math.max(1, Math.floor((wrap.clientWidth - HEADER_WIDTH) / COL_WIDTH));
  const fullRows = Math.max(1, Math.floor((wrap.clientHeight - HEADER_HEIGHT) / ROW_HEIGHT));
  if (selection.col < origin.col) origin.col = selection.col;
  if (selection.col >= origin.col + fullCols) origin.col = selection.col - fullCols + 1;
  if (selection.row < origin.row) origin.row = selection.row;
  if (selection.row >= origin.row + fullRows) origin.row = selection.row - fullRows + 1;
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

  const { cols, rows } = visibleSize();
  const x = (col: number) => HEADER_WIDTH + (col - origin.col) * COL_WIDTH;
  const y = (row: number) => HEADER_HEIGHT + (row - origin.row) * ROW_HEIGHT;

  ctx.fillStyle = "#fff";
  ctx.fillRect(0, 0, width, height);

  // Linhas de grade
  ctx.strokeStyle = COLORS.line;
  ctx.lineWidth = 1;
  ctx.beginPath();
  for (let c = origin.col; c <= origin.col + cols; c++) {
    ctx.moveTo(x(c) + 0.5, 0);
    ctx.lineTo(x(c) + 0.5, height);
  }
  for (let r = origin.row; r <= origin.row + rows; r++) {
    ctx.moveTo(0, y(r) + 0.5);
    ctx.lineTo(width, y(r) + 0.5);
  }
  ctx.stroke();

  // Fundos coloridos, antes do texto. Como no Excel, cobrem as linhas de
  // grade à direita e abaixo da célula.
  for (const cell of cells.values()) {
    if (!cell.style?.fill) continue;
    ctx.fillStyle = cell.style.fill;
    ctx.fillRect(x(cell.col) + 1, y(cell.row) + 1, COL_WIDTH, ROW_HEIGHT);
  }

  // Conteúdo das células
  ctx.textBaseline = "middle";
  for (const cell of cells.values()) {
    if (cell.display === "") continue;
    const style = cell.style;
    const left = x(cell.col);
    const middle = y(cell.row) + ROW_HEIGHT / 2 + 1;
    const align = style?.align ?? (cell.kind === "number" ? "right" : cell.kind === "error" ? "center" : "left");
    const anchor = align === "right" ? left + COL_WIDTH - 8 : align === "center" ? left + COL_WIDTH / 2 : left + 8;

    ctx.save();
    ctx.font = `${style?.italic ? "italic " : ""}${style?.bold ? "700 " : ""}14px ${FONT}`;
    // Como no Excel, texto alinhado à esquerda invade as células vazias à direita.
    let span = 1;
    if (cell.kind === "text" && align === "left") {
      const needed = ctx.measureText(cell.display).width + 16;
      while (needed > span * COL_WIDTH && !cells.get(key(cell.col + span, cell.row))?.display) span++;
    }
    ctx.beginPath();
    ctx.rect(left + 1, y(cell.row) + 1, span * COL_WIDTH - 1, ROW_HEIGHT - 1);
    ctx.clip();
    ctx.fillStyle = style?.color ?? (cell.kind === "error" ? COLORS.error : COLORS.ink);
    ctx.textAlign = align;
    ctx.fillText(cell.display, anchor, middle);
    if (style?.underline || style?.strike) {
      const textWidth = ctx.measureText(cell.display).width;
      const start = align === "right" ? anchor - textWidth : align === "center" ? anchor - textWidth / 2 : anchor;
      if (style.underline) ctx.fillRect(start, middle + 8, textWidth, 1);
      if (style.strike) ctx.fillRect(start, middle, textWidth, 1);
    }
    ctx.restore();
  }

  // Cabeçalhos, por cima das células
  ctx.fillStyle = COLORS.header;
  ctx.fillRect(0, 0, width, HEADER_HEIGHT);
  ctx.fillRect(0, 0, HEADER_WIDTH, height);
  ctx.fillStyle = COLORS.headerActive;
  ctx.fillRect(x(selection.col), 0, COL_WIDTH, HEADER_HEIGHT);
  ctx.fillRect(0, y(selection.row), HEADER_WIDTH, ROW_HEIGHT);
  ctx.fillStyle = COLORS.header;
  ctx.fillRect(0, 0, HEADER_WIDTH, HEADER_HEIGHT);

  ctx.strokeStyle = COLORS.line;
  ctx.beginPath();
  for (let c = origin.col; c <= origin.col + cols; c++) {
    ctx.moveTo(x(c) + 0.5, 0);
    ctx.lineTo(x(c) + 0.5, HEADER_HEIGHT);
  }
  for (let r = origin.row; r <= origin.row + rows; r++) {
    ctx.moveTo(0, y(r) + 0.5);
    ctx.lineTo(HEADER_WIDTH, y(r) + 0.5);
  }
  ctx.moveTo(0, HEADER_HEIGHT + 0.5);
  ctx.lineTo(width, HEADER_HEIGHT + 0.5);
  ctx.moveTo(HEADER_WIDTH + 0.5, 0);
  ctx.lineTo(HEADER_WIDTH + 0.5, height);
  ctx.stroke();

  ctx.font = `13px ${FONT}`;
  ctx.textAlign = "center";
  for (let c = origin.col; c < origin.col + cols && c < MAX_COLS; c++) {
    ctx.fillStyle = c === selection.col ? COLORS.headerActiveInk : COLORS.muted;
    ctx.fillText(colName(c), x(c) + COL_WIDTH / 2, HEADER_HEIGHT / 2);
  }
  for (let r = origin.row; r < origin.row + rows && r < MAX_ROWS; r++) {
    ctx.fillStyle = r === selection.row ? COLORS.headerActiveInk : COLORS.muted;
    ctx.fillText(String(r + 1), HEADER_WIDTH / 2, y(r) + ROW_HEIGHT / 2);
  }

  // Seleção
  ctx.strokeStyle = COLORS.accent;
  ctx.lineWidth = 2;
  ctx.strokeRect(x(selection.col) + 0.5, y(selection.row) + 0.5, COL_WIDTH, ROW_HEIGHT);
}

/** Mostra a célula selecionada na caixa de nome e na barra de fórmulas. */
function syncFormulaBar() {
  nameBox.textContent = colName(selection.col) + (selection.row + 1);
  fx.value = cells.get(key(selection.col, selection.row))?.input ?? "";
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

/** Busca no motor as células da janela visível e redesenha. */
async function refresh() {
  try {
    const { cols, rows } = visibleSize();
    const view = await engine.view(sheet, origin.col, origin.row, cols, rows);
    cells = new Map(view.map((cell) => [key(cell.col, cell.row), cell]));
  } catch (error) {
    statusEl.textContent = `Erro ao ler a planilha: ${error}`;
  }
  draw();
  syncFormulaBar();
}

async function select(col: number, row: number) {
  selection.col = clamp(col, MAX_COLS - 1);
  selection.row = clamp(row, MAX_ROWS - 1);
  scrollToSelection();
  await refresh();
}

/** Grava o texto da barra de fórmulas na célula selecionada. */
async function write(): Promise<boolean> {
  const current = cells.get(key(selection.col, selection.row))?.input ?? "";
  if (fx.value === current) {
    dirty = false;
    return true;
  }
  try {
    await engine.setCell(sheet, selection.col, selection.row, fx.value);
    dirty = false;
    statusEl.textContent = "Pronto";
    markModified();
    return true;
  } catch (error) {
    statusEl.textContent = `Erro ao gravar a célula: ${error}`;
    return false;
  }
}

/** Grava a edição e move a seleção. */
async function commit(moveCols: number, moveRows: number) {
  if (!(await write())) return;
  canvas.focus();
  await select(selection.col + moveCols, selection.row + moveRows);
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
    ({ selection, origin } = positions.get(sheet) ?? newPosition());
  }
  renderTabs();
}

/** Troca a planilha na tela, guardando a posição da anterior. */
async function showSheet(index: number) {
  if (dirty && !(await write())) return;
  positions.set(sheet, { selection, origin });
  sheet = index;
  ({ selection, origin } = positions.get(index) ?? newPosition());
  renderTabs();
  canvas.focus();
  await refresh();
}

async function addSheet() {
  if (dirty && !(await write())) return;
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
  if (dirty && !(await write())) return;
  try {
    if (modified && !(await files.confirmDiscard())) return;
    const path = await files.open();
    if (path === null) return;
    filePath = path;
    modified = false;
    positions = new Map();
    sheet = 0;
    ({ selection, origin } = newPosition());
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
  if (dirty && !(await write())) return;
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

canvas.addEventListener("mousedown", (event) => {
  const bounds = canvas.getBoundingClientRect();
  const px = event.clientX - bounds.left;
  const py = event.clientY - bounds.top;
  if (px < HEADER_WIDTH || py < HEADER_HEIGHT) return;
  const col = origin.col + Math.floor((px - HEADER_WIDTH) / COL_WIDTH);
  const row = origin.row + Math.floor((py - HEADER_HEIGHT) / ROW_HEIGHT);
  // Clicar em outra célula no meio de uma edição grava a edição, como no Excel.
  void (async () => {
    if (dirty && !(await write())) return;
    await select(col, row);
  })();
});

canvas.addEventListener(
  "wheel",
  (event) => {
    event.preventDefault();
    const horizontal = event.shiftKey || Math.abs(event.deltaX) > Math.abs(event.deltaY);
    const delta = horizontal ? event.deltaX || event.deltaY : event.deltaY;
    if (delta === 0) return;
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
    Tab: [event.shiftKey ? -1 : 1, 0],
  };
  const move = moves[event.key];
  const shortcut = event.ctrlKey || event.metaKey;
  if (shortcut && event.key.toLowerCase() === "z") {
    event.preventDefault();
    void history(event.shiftKey ? "redo" : "undo");
  } else if (shortcut && event.key.toLowerCase() === "y") {
    event.preventDefault();
    void history("redo");
  } else if (move) {
    event.preventDefault();
    void select(selection.col + move[0], selection.row + move[1]);
  } else if (event.key === "Enter" || event.key === "F2") {
    event.preventDefault();
    fx.focus();
  } else if (event.key === "Delete" || event.key === "Backspace") {
    event.preventDefault();
    fx.value = "";
    void commit(0, 0);
  } else if (event.key.length === 1 && !event.ctrlKey && !event.metaKey && !event.altKey) {
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

$("#open-btn").addEventListener("click", () => void openFile());
$("#save-btn").addEventListener("click", () => void saveFile(false));
$("#save-as-btn").addEventListener("click", () => void saveFile(true));
$("#add-sheet").addEventListener("click", () => void addSheet());
$("#file-actions").hidden = engine.files === null;

new ResizeObserver(() => void refresh()).observe(wrap);

syncDocument();
canvas.focus();
void loadSheets().then(refresh);
