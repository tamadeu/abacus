import { invoke, isTauri } from "@tauri-apps/api/core";

/** O que a interface precisa saber para desenhar uma célula. */
export interface CellView {
  col: number;
  row: number;
  /** Texto digitado (o que aparece na barra de fórmulas). */
  input: string;
  /** Texto mostrado na grade. */
  display: string;
  kind: "number" | "text" | "error";
}

/**
 * Ponte entre a interface e o motor de planilhas. A interface só conhece
 * esta interface, então trocar a casca (Tauri hoje, WebAssembly na versão
 * web) não muda o resto do código.
 */
export interface Engine {
  readonly label: string;
  setCell(col: number, row: number, input: string): Promise<void>;
  /** Células preenchidas dentro da janela visível, já calculadas. */
  view(firstCol: number, firstRow: number, cols: number, rows: number): Promise<CellView[]>;
}

/** Motor em Rust, chamado pelos comandos do Tauri. */
class TauriEngine implements Engine {
  readonly label = "Planilha1";

  setCell(col: number, row: number, input: string): Promise<void> {
    return invoke("set_cell", { col, row, input });
  }

  view(firstCol: number, firstRow: number, cols: number, rows: number): Promise<CellView[]> {
    return invoke("get_view", { firstCol, firstRow, cols, rows });
  }
}

/**
 * Substituto usado quando a interface roda no navegador (`npm run dev`),
 * fora do Tauri. Guarda o que foi digitado, mas não calcula fórmulas.
 */
class PreviewEngine implements Engine {
  readonly label = "Pré-visualização no navegador (sem motor de cálculo)";
  private cells = new Map<string, CellView>();

  async setCell(col: number, row: number, input: string): Promise<void> {
    const key = `${col},${row}`;
    if (input.trim() === "") {
      this.cells.delete(key);
      return;
    }
    const isNumber = /^-?\d+([.,]\d+)?$/.test(input.trim());
    this.cells.set(key, { col, row, input, display: input, kind: isNumber ? "number" : "text" });
  }

  async view(firstCol: number, firstRow: number, cols: number, rows: number): Promise<CellView[]> {
    return [...this.cells.values()].filter(
      (c) => c.col >= firstCol && c.col < firstCol + cols && c.row >= firstRow && c.row < firstRow + rows,
    );
  }
}

export function createEngine(): Engine {
  return isTauri() ? new TauriEngine() : new PreviewEngine();
}
