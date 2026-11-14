import { createEngine, type CellView } from "./engine";

const COL_WIDTH = 110;
const ROW_HEIGHT = 26;
const HEADER_WIDTH = 48;
const HEADER_HEIGHT = 26;
// Limites de uma planilha, iguais aos do Excel e aos do núcleo em Rust.
const MAX_COLS = 16_384;
const MAX_ROWS = 1_048_576;

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
const wrap = document.querySelector<HTMLElement>("#grid-wrap")!;
const canvas = document.querySelector<HTMLCanvasElement>("#grid")!;
const ctx = canvas.getContext("2d")!;
const nameBox = document.querySelector<HTMLElement>("#name-box")!;
const fx = document.querySelector<HTMLInputElement>("#fx")!;
const statusEl = document.querySelector<HTMLElement>("#status")!;

/** Célula selecionada. */
const selection = { col: 0, row: 0 };
/** Primeira coluna e linha visíveis (a rolagem é por célula inteira). */
const origin = { col: 0, row: 0 };
/** Células preenchidas da janela visível, por "coluna,linha". */
let cells = new Map<string, CellView>();
/** Há texto na barra de fórmulas que ainda não foi gravado na célula. */
let dirty = false;

const key = (col: number, row: number) => `${col},${row}`;

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

  // Cabeçalhos
  ctx.fillStyle = COLORS.header;
  ctx.fillRect(0, 0, width, HEADER_HEIGHT);
  ctx.fillRect(0, 0, HEADER_WIDTH, height);
  ctx.fillStyle = COLORS.headerActive;
  ctx.fillRect(x(selection.col), 0, COL_WIDTH, HEADER_HEIGHT);
  ctx.fillRect(0, y(selection.row), HEADER_WIDTH, ROW_HEIGHT);

  ctx.font = "13px Inter, system-ui, sans-serif";
  ctx.textBaseline = "middle";
  ctx.textAlign = "center";
  for (let c = origin.col; c < origin.col + cols && c < MAX_COLS; c++) {
    ctx.fillStyle = c === selection.col ? COLORS.headerActiveInk : COLORS.muted;
    ctx.fillText(colName(c), x(c) + COL_WIDTH / 2, HEADER_HEIGHT / 2);
  }
  for (let r = origin.row; r < origin.row + rows && r < MAX_ROWS; r++) {
    ctx.fillStyle = r === selection.row ? COLORS.headerActiveInk : COLORS.muted;
    ctx.fillText(String(r + 1), HEADER_WIDTH / 2, y(r) + ROW_HEIGHT / 2);
  }

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

  // Conteúdo das células
  ctx.font = "14px Inter, system-ui, sans-serif";
  for (const cell of cells.values()) {
    const left = x(cell.col);
    const top = y(cell.row);
    ctx.save();
    ctx.beginPath();
    ctx.rect(left + 1, top + 1, COL_WIDTH - 1, ROW_HEIGHT - 1);
    ctx.clip();
    ctx.fillStyle = cell.kind === "error" ? COLORS.error : COLORS.ink;
    if (cell.kind === "number") {
      ctx.textAlign = "right";
      ctx.fillText(cell.display, left + COL_WIDTH - 8, top + ROW_HEIGHT / 2 + 1);
    } else if (cell.kind === "error") {
      ctx.textAlign = "center";
      ctx.fillText(cell.display, left + COL_WIDTH / 2, top + ROW_HEIGHT / 2 + 1);
    } else {
      ctx.textAlign = "left";
      ctx.fillText(cell.display, left + 8, top + ROW_HEIGHT / 2 + 1);
    }
    ctx.restore();
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

/** Busca no motor as células da janela visível e redesenha. */
async function refresh() {
  try {
    const { cols, rows } = visibleSize();
    const view = await engine.view(origin.col, origin.row, cols, rows);
    cells = new Map(view.map((cell) => [key(cell.col, cell.row), cell]));
    statusEl.textContent = "Pronto";
  } catch (error) {
    statusEl.textContent = `Erro ao ler a planilha: ${error}`;
  }
  draw();
  syncFormulaBar();
}

async function select(col: number, row: number) {
  selection.col = Math.min(Math.max(col, 0), MAX_COLS - 1);
  selection.row = Math.min(Math.max(row, 0), MAX_ROWS - 1);
  scrollToSelection();
  await refresh();
}

/** Grava o texto da barra de fórmulas na célula selecionada. */
async function write(): Promise<boolean> {
  try {
    await engine.setCell(selection.col, selection.row, fx.value);
    dirty = false;
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
      origin.col = Math.min(Math.max(origin.col + Math.sign(delta), 0), MAX_COLS - 1);
    } else {
      origin.row = Math.min(Math.max(origin.row + Math.sign(delta) * 3, 0), MAX_ROWS - 1);
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
  if (move) {
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

new ResizeObserver(() => void refresh()).observe(wrap);

document.querySelector<HTMLElement>("#engine-label")!.textContent = engine.label;
canvas.focus();
void refresh();
