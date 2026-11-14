import type { Range } from "./engine";

// Limites de uma planilha, iguais aos do Excel.
const MAX_COLS = 16_384;
const MAX_ROWS = 1_048_576;

/**
 * Uma referência no estilo A1, com `$` opcionais, sozinha ou como intervalo
 * (`B3`, `$B$3`, `B3:D6`). Não casa com:
 * - pedaços de nomes (`LOG10(`, `TAXA2`), por causa das bordas;
 * - referências a outra planilha (`Resumo!B3`), que não estão na tela.
 */
const REFERENCE = /(?<![\p{L}\p{N}_.!'$])\$?([A-Za-z]{1,3})\$?(\d{1,7})(?::\$?([A-Za-z]{1,3})\$?(\d{1,7}))?(?![\p{L}\p{N}_.(!])/gu;

function colIndex(letters: string): number {
  let col = 0;
  for (const letter of letters.toUpperCase()) col = col * 26 + letter.charCodeAt(0) - 64;
  return col - 1;
}

/**
 * Intervalos da planilha atual citados numa fórmula, na ordem em que
 * aparecem e sem repetição. O texto entre aspas é ignorado.
 */
export function formulaRefs(formula: string): Range[] {
  // Troca os textos entre aspas por espaços, mantendo as posições.
  const code = formula.replace(/"[^"]*"?/g, (text) => " ".repeat(text.length));
  const refs = new Map<string, Range>();
  for (const match of code.matchAll(REFERENCE)) {
    const [, col1, row1, col2 = col1, row2 = row1] = match;
    const cols = [colIndex(col1), colIndex(col2)].sort((a, b) => a - b);
    const rows = [Number(row1) - 1, Number(row2) - 1].sort((a, b) => a - b);
    if (cols[1] >= MAX_COLS || rows[0] < 0 || rows[1] >= MAX_ROWS) continue;
    const range = { col: cols[0], row: rows[0], cols: cols[1] - cols[0] + 1, rows: rows[1] - rows[0] + 1 };
    refs.set(`${range.col},${range.row},${range.cols},${range.rows}`, range);
  }
  return [...refs.values()];
}
