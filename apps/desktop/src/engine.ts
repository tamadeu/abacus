import { invoke, isTauri } from "@tauri-apps/api/core";
import { ask, open, save } from "@tauri-apps/plugin-dialog";

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
}

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
}

/** O pedaço de uma planilha que cabe na tela. */
export interface View {
  /** Largura, em pixels, de cada coluna visível a partir da primeira. */
  colWidths: number[];
  /** Altura, em pixels, de cada linha visível a partir da primeira. */
  rowHeights: number[];
  /** Células com conteúdo ou estilo, já calculadas. */
  cells: CellView[];
}

/** Um retângulo de células, com coluna e linha iniciais a partir de zero. */
export interface Range {
  col: number;
  row: number;
  cols: number;
  rows: number;
}

/**
 * Propriedades de estilo que a interface pode alterar. `bold`, `italic`,
 * `underline` e `strike` recebem `"true"` ou `"false"`; `color` e `fill`,
 * uma cor `#RRGGBB` ou vazio para remover; `align`, `"left"`, `"center"`,
 * `"right"` ou `"general"`; `format`, um formato numérico do Excel.
 */
export type StyleProperty = "bold" | "italic" | "underline" | "strike" | "color" | "fill" | "align" | "format";

/** Uma planilha visível da pasta de trabalho. */
export interface SheetInfo {
  /** Índice usado nas demais chamadas. */
  index: number;
  name: string;
}

/** Abrir e salvar arquivos, com os diálogos do sistema. */
export interface FileAccess {
  /** Pergunta qual arquivo abrir e o carrega. Devolve o caminho, ou `null` se a pessoa cancelou. */
  open(): Promise<string | null>;
  /** Salva em `path`; sem caminho, pergunta onde. Devolve o caminho usado, ou `null` se a pessoa cancelou. */
  save(path: string | null): Promise<string | null>;
  /** Pergunta se as alterações não salvas podem ser descartadas. */
  confirmDiscard(): Promise<boolean>;
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
  sheets(): Promise<SheetInfo[]>;
  /** Cria uma planilha vazia e devolve o índice dela. */
  addSheet(): Promise<number>;
  setCell(sheet: number, col: number, row: number, input: string): Promise<void>;
  /** O que cabe numa tela de `width` por `height` pixels a partir da célula dada. */
  view(sheet: number, firstCol: number, firstRow: number, width: number, height: number): Promise<View>;
  /** Altera uma propriedade de estilo nas células do intervalo. */
  format(sheet: number, range: Range, property: StyleProperty, value: string): Promise<void>;
  /** Apaga o conteúdo das células do intervalo, mantendo o estilo. */
  clearCells(sheet: number, range: Range): Promise<void>;
  undo(): Promise<void>;
  redo(): Promise<void>;
}

const XLSX_FILTER = [{ name: "Pasta de trabalho do Excel", extensions: ["xlsx"] }];

/** Motor IronCalc, em Rust, chamado pelos comandos do Tauri. */
class TauriEngine implements Engine {
  readonly notice = null;
  readonly formatting = true;

  readonly files: FileAccess = {
    async open() {
      const path = await open({ multiple: false, directory: false, filters: XLSX_FILTER });
      if (path === null) return null;
      await invoke("open_file", { path });
      return path;
    },
    async save(path) {
      let target = path ?? (await save({ filters: XLSX_FILTER, defaultPath: "Sem título.xlsx" }));
      if (target === null) return null;
      if (!target.toLowerCase().endsWith(".xlsx")) target += ".xlsx";
      await invoke("save_file", { path: target });
      return target;
    },
    confirmDiscard() {
      return ask("As alterações não salvas serão perdidas. Continuar?", {
        title: "Ábaco",
        kind: "warning",
        okLabel: "Descartar alterações",
        cancelLabel: "Cancelar",
      });
    },
  };

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
 * formata, não desfaz edições e não abre nem salva arquivos.
 */
class PreviewEngine implements Engine {
  readonly notice = "Pré-visualização no navegador (sem motor de cálculo)";
  readonly files = null;
  readonly formatting = false;
  private names = ["Sheet1"];
  private cells = new Map<string, CellView>();

  async sheets(): Promise<SheetInfo[]> {
    return this.names.map((name, index) => ({ index, name }));
  }

  async addSheet(): Promise<number> {
    this.names.push(`Sheet${this.names.length + 1}`);
    return this.names.length - 1;
  }

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
      colWidths: new Array<number>(cols).fill(90),
      rowHeights: new Array<number>(rows).fill(25),
      cells: this.inRange(sheet, range).map(([, cell]) => cell),
    };
  }

  async format(): Promise<void> {}

  async clearCells(sheet: number, range: Range): Promise<void> {
    for (const [key] of this.inRange(sheet, range)) this.cells.delete(key);
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

  async undo(): Promise<void> {}

  async redo(): Promise<void> {}
}

export function createEngine(): Engine {
  return isTauri() ? new TauriEngine() : new PreviewEngine();
}
