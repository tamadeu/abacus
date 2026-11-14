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
  sheets(): Promise<SheetInfo[]>;
  /** Cria uma planilha vazia e devolve o índice dela. */
  addSheet(): Promise<number>;
  setCell(sheet: number, col: number, row: number, input: string): Promise<void>;
  /** Células com conteúdo ou estilo dentro da janela visível, já calculadas. */
  view(sheet: number, firstCol: number, firstRow: number, cols: number, rows: number): Promise<CellView[]>;
  undo(): Promise<void>;
  redo(): Promise<void>;
}

const XLSX_FILTER = [{ name: "Pasta de trabalho do Excel", extensions: ["xlsx"] }];

/** Motor IronCalc, em Rust, chamado pelos comandos do Tauri. */
class TauriEngine implements Engine {
  readonly notice = null;

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

  view(sheet: number, firstCol: number, firstRow: number, cols: number, rows: number): Promise<CellView[]> {
    return invoke("get_view", { sheet, firstCol, firstRow, cols, rows });
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
 * desfaz edições e não abre nem salva arquivos.
 */
class PreviewEngine implements Engine {
  readonly notice = "Pré-visualização no navegador (sem motor de cálculo)";
  readonly files = null;
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

  async view(sheet: number, firstCol: number, firstRow: number, cols: number, rows: number): Promise<CellView[]> {
    const inWindow = (c: CellView) =>
      c.col >= firstCol && c.col < firstCol + cols && c.row >= firstRow && c.row < firstRow + rows;
    return [...this.cells]
      .filter(([key, cell]) => key.startsWith(`${sheet}:`) && inWindow(cell))
      .map(([, cell]) => cell);
  }

  async undo(): Promise<void> {}

  async redo(): Promise<void> {}
}

export function createEngine(): Engine {
  return isTauri() ? new TauriEngine() : new PreviewEngine();
}
