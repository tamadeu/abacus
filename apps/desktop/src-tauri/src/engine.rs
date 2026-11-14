//! Ponte entre o Ábaco e o motor de planilhas [IronCalc](https://www.ironcalc.com).
//!
//! A interface usa colunas e linhas contadas a partir de zero; o IronCalc
//! conta a partir de um. A conversão acontece só aqui.

use ironcalc::base::UserModel;
use ironcalc::base::types::CellType;
use serde::Serialize;

/// Localidade dos números e datas.
///
/// O IronCalc 0.9 ainda não tem português. O espanhol é o substituto mais
/// próximo: vírgula decimal, ponto de milhar, `;` entre argumentos e datas
/// em dia/mês/ano. Trocar para `pt-BR` quando existir.
const LOCALE: &str = "es";
/// Idioma dos nomes de função e dos erros (`SUM`, `#DIV/0!`). Também não há
/// português ainda, então fica em inglês.
const LANGUAGE: &str = "en";
const TIMEZONE: &str = "America/Sao_Paulo";
/// Única planilha usada por enquanto.
const SHEET: u32 = 0;

/// O que a interface precisa saber para desenhar uma célula.
#[derive(Debug, Serialize)]
pub struct CellView {
    pub col: u32,
    pub row: u32,
    /// Conteúdo editável (o que aparece na barra de fórmulas).
    pub input: String,
    /// Texto mostrado na grade, já formatado.
    pub display: String,
    pub kind: CellKind,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CellKind {
    Number,
    Text,
    Error,
}

/// A pasta de trabalho aberta.
pub struct Engine {
    model: UserModel<'static>,
}

impl Engine {
    /// Pasta nova, vazia.
    pub fn new() -> Result<Self, String> {
        let model = UserModel::new_empty("Pasta1", LOCALE, TIMEZONE, LANGUAGE)?;
        Ok(Self { model })
    }

    /// Grava o conteúdo de uma célula. Texto vazio apaga a célula.
    pub fn set_cell(&mut self, col: u32, row: u32, input: &str) -> Result<(), String> {
        let (row, col) = to_ironcalc(col, row)?;
        let input = if input.trim().is_empty() { "" } else { input };
        self.model.set_user_input(SHEET, row, col, input)
    }

    /// Células preenchidas dentro da janela visível, já calculadas.
    pub fn view(&self, first_col: u32, first_row: u32, cols: u32, rows: u32) -> Result<Vec<CellView>, String> {
        // Só percorre a parte da janela que tem dados.
        let used = self.model.get_model().workbook.worksheet(SHEET)?.dimension();
        let (top, left) = to_ironcalc(first_col, first_row)?;
        let bottom = top.saturating_add(to_i32(rows)?).saturating_sub(1).min(used.max_row);
        let right = left.saturating_add(to_i32(cols)?).saturating_sub(1).min(used.max_column);

        let mut view = Vec::new();
        for row in top.max(used.min_row)..=bottom {
            for col in left.max(used.min_column)..=right {
                let input = self.model.get_cell_content(SHEET, row, col)?;
                let display = self.model.get_formatted_cell_value(SHEET, row, col)?;
                if input.is_empty() && display.is_empty() {
                    continue;
                }
                let kind = match self.model.get_cell_type(SHEET, row, col)? {
                    CellType::Number => CellKind::Number,
                    CellType::ErrorValue => CellKind::Error,
                    _ => CellKind::Text,
                };
                view.push(CellView { col: (col - 1) as u32, row: (row - 1) as u32, input, display, kind });
            }
        }
        Ok(view)
    }

    /// Desfaz a última edição, se houver.
    pub fn undo(&mut self) -> Result<(), String> {
        if self.model.can_undo() { self.model.undo() } else { Ok(()) }
    }

    /// Refaz a última edição desfeita, se houver.
    pub fn redo(&mut self) -> Result<(), String> {
        if self.model.can_redo() { self.model.redo() } else { Ok(()) }
    }
}

fn to_i32(value: u32) -> Result<i32, String> {
    i32::try_from(value).map_err(|_| format!("valor fora da planilha: {value}"))
}

/// Converte coluna e linha a partir de zero para (linha, coluna) a partir de um.
fn to_ironcalc(col: u32, row: u32) -> Result<(i32, i32), String> {
    Ok((to_i32(row)?.saturating_add(1), to_i32(col)?.saturating_add(1)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Preenche células dadas como (coluna, linha, texto), a partir de zero.
    fn engine(cells: &[(u32, u32, &str)]) -> Engine {
        let mut engine = Engine::new().unwrap();
        for (col, row, input) in cells {
            engine.set_cell(*col, *row, input).unwrap();
        }
        engine
    }

    fn cell(engine: &Engine, col: u32, row: u32) -> Option<CellView> {
        engine.view(col, row, 1, 1).unwrap().pop()
    }

    fn shown(engine: &Engine, col: u32, row: u32) -> String {
        cell(engine, col, row).map(|c| c.display).unwrap_or_default()
    }

    #[test]
    fn financial_report() {
        // Colunas B, C, D nas linhas 3 e 4, mais os totais na linha 8.
        let e = engine(&[
            (0, 2, "Janeiro"),
            (1, 2, "18.500,00"),
            (2, 2, "9.200,00"),
            (3, 2, "=B3-C3"),
            (1, 3, "22.400,00"),
            (2, 3, "11.800,00"),
            (3, 3, "=B4-C4"),
            (1, 7, "=SUM(B3:B6)"),
            (3, 7, "=SUM(D3:D6)"),
        ]);
        assert_eq!(shown(&e, 3, 2), "9.300,00");
        assert_eq!(shown(&e, 3, 3), "10.600,00");
        assert_eq!(shown(&e, 1, 7), "40.900,00");
        assert_eq!(shown(&e, 3, 7), "19.900,00");
    }

    #[test]
    fn view_reports_input_display_and_kind() {
        let e = engine(&[(0, 0, "Janeiro"), (1, 0, "1,5"), (2, 0, "=B1*2"), (3, 0, "=B1/0")]);
        let text = cell(&e, 0, 0).unwrap();
        assert_eq!((text.input.as_str(), text.display.as_str(), &text.kind), ("Janeiro", "Janeiro", &CellKind::Text));
        let formula = cell(&e, 2, 0).unwrap();
        assert_eq!((formula.input.as_str(), formula.display.as_str(), &formula.kind), ("=B1*2", "3", &CellKind::Number));
        let error = cell(&e, 3, 0).unwrap();
        assert_eq!((error.display.as_str(), &error.kind), ("#DIV/0!", &CellKind::Error));
    }

    #[test]
    fn view_returns_only_cells_inside_the_window() {
        let e = engine(&[(0, 0, "1"), (5, 5, "2"), (20, 40, "3")]);
        let positions = |view: Vec<CellView>| view.into_iter().map(|c| (c.col, c.row)).collect::<Vec<_>>();
        assert_eq!(positions(e.view(0, 0, 10, 10).unwrap()), [(0, 0), (5, 5)]);
        assert_eq!(positions(e.view(5, 5, 30, 40).unwrap()), [(5, 5), (20, 40)]);
        assert_eq!(positions(e.view(6, 6, 5, 5).unwrap()), []);
        assert_eq!(positions(e.view(100, 100, 10, 10).unwrap()), []);
    }

    #[test]
    fn empty_input_clears_the_cell() {
        let mut e = engine(&[(0, 0, "10"), (1, 0, "=A1*2")]);
        assert_eq!(shown(&e, 1, 0), "20");
        e.set_cell(0, 0, "   ").unwrap();
        assert!(cell(&e, 0, 0).is_none());
        assert_eq!(shown(&e, 1, 0), "0");
    }

    #[test]
    fn undo_and_redo() {
        let mut e = engine(&[(0, 0, "10")]);
        e.set_cell(0, 0, "25").unwrap();
        e.undo().unwrap();
        assert_eq!(shown(&e, 0, 0), "10");
        e.redo().unwrap();
        assert_eq!(shown(&e, 0, 0), "25");
        // Sem nada para refazer, não é erro.
        e.redo().unwrap();
        assert_eq!(shown(&e, 0, 0), "25");
    }

    #[test]
    fn modern_excel_features() {
        let e = engine(&[(0, 0, "=LET(x;2;x*3)"), (1, 0, "=LAMBDA(x;x+1)(2)"), (2, 0, "=SEQUENCE(3)")]);
        assert_eq!(shown(&e, 0, 0), "6");
        assert_eq!(shown(&e, 1, 0), "3");
        // Matriz dinâmica: o resultado transborda para as células de baixo.
        assert_eq!([shown(&e, 2, 0), shown(&e, 2, 1), shown(&e, 2, 2)], ["1", "2", "3"]);
    }

    #[test]
    fn rejects_positions_outside_the_sheet() {
        let mut e = Engine::new().unwrap();
        assert!(e.set_cell(16_384, 0, "1").is_err());
        assert!(e.set_cell(0, 1_048_576, "1").is_err());
        assert!(e.set_cell(16_383, 1_048_575, "1").is_ok());
    }
}
