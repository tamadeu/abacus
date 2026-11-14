//! Ponte entre o Ábaco e o motor de planilhas [IronCalc](https://www.ironcalc.com).
//!
//! A interface usa planilhas, colunas e linhas contadas a partir de zero; o
//! IronCalc conta linhas e colunas a partir de um. A conversão acontece só
//! aqui.

use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use ironcalc::base::UserModel;
use ironcalc::base::types::{CellType, HorizontalAlignment, Style};
use ironcalc::export::save_xlsx_to_writer;
use ironcalc::import::load_from_xlsx;
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
    /// Ausente quando a célula usa o estilo padrão.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<CellStyle>,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CellKind {
    Number,
    Text,
    Error,
}

/// Parte do estilo de uma célula que a grade sabe desenhar.
#[derive(Debug, Default, PartialEq, Eq, Serialize)]
pub struct CellStyle {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    /// Cor do texto, como `#RRGGBB`.
    pub color: Option<String>,
    /// Cor de fundo, como `#RRGGBB`.
    pub fill: Option<String>,
    /// Alinhamento horizontal explícito. Sem ele, números ficam à direita e
    /// texto à esquerda.
    pub align: Option<Align>,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Align {
    Left,
    Center,
    Right,
}

/// Uma planilha visível da pasta de trabalho.
#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct SheetInfo {
    /// Índice usado nas demais chamadas.
    pub index: u32,
    pub name: String,
}

/// A pasta de trabalho aberta.
pub struct Engine {
    model: UserModel<'static>,
}

impl Engine {
    /// Pasta nova, com uma planilha vazia.
    pub fn new() -> Result<Self, String> {
        let model = UserModel::new_empty("Pasta1", LOCALE, TIMEZONE, LANGUAGE)?;
        Ok(Self { model })
    }

    /// Abre um arquivo `.xlsx`.
    pub fn open(path: &Path) -> Result<Self, String> {
        let file_name = path.to_str().ok_or("o caminho do arquivo não é um texto válido")?;
        let model = load_from_xlsx(file_name, LOCALE, TIMEZONE, LANGUAGE).map_err(|e| e.to_string())?;
        let mut model = UserModel::from_model(model);
        model.evaluate();
        Ok(Self { model })
    }

    /// Salva a pasta como `.xlsx`, substituindo o arquivo se já existir.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let buffer = save_xlsx_to_writer(self.model.get_model(), Cursor::new(Vec::new()))
            .map_err(|e| e.to_string())?
            .into_inner();
        // Grava num arquivo temporário e troca no fim, para uma falha no meio
        // do caminho não estragar o arquivo que já existia.
        let mut temp = path.as_os_str().to_owned();
        temp.push(".tmp");
        let temp = PathBuf::from(temp);
        fs::write(&temp, buffer).map_err(|e| format!("não foi possível gravar o arquivo: {e}"))?;
        fs::rename(&temp, path).map_err(|e| {
            let _ = fs::remove_file(&temp);
            format!("não foi possível gravar o arquivo: {e}")
        })
    }

    /// Planilhas visíveis, na ordem das abas.
    pub fn sheets(&self) -> Vec<SheetInfo> {
        self.model
            .get_worksheets_properties()
            .into_iter()
            .enumerate()
            .filter(|(_, sheet)| sheet.state == "visible")
            .map(|(index, sheet)| SheetInfo { index: index as u32, name: sheet.name })
            .collect()
    }

    /// Cria uma planilha vazia no fim e devolve o índice dela.
    pub fn add_sheet(&mut self) -> Result<u32, String> {
        self.model.new_sheet()?;
        Ok(self.model.get_selected_sheet())
    }

    /// Grava o conteúdo de uma célula. Texto vazio apaga a célula.
    pub fn set_cell(&mut self, sheet: u32, col: u32, row: u32, input: &str) -> Result<(), String> {
        let (row, col) = to_ironcalc(col, row)?;
        let input = if input.trim().is_empty() { "" } else { input };
        self.model.set_user_input(sheet, row, col, input)
    }

    /// Células com conteúdo ou estilo dentro da janela visível, já calculadas.
    pub fn view(
        &self,
        sheet: u32,
        first_col: u32,
        first_row: u32,
        cols: u32,
        rows: u32,
    ) -> Result<Vec<CellView>, String> {
        // Só percorre a parte da janela que tem dados.
        let used = self.model.get_model().workbook.worksheet(sheet)?.dimension();
        let (top, left) = to_ironcalc(first_col, first_row)?;
        let bottom = top.saturating_add(to_i32(rows)?).saturating_sub(1).min(used.max_row);
        let right = left.saturating_add(to_i32(cols)?).saturating_sub(1).min(used.max_column);

        let mut view = Vec::new();
        for row in top.max(used.min_row)..=bottom {
            for col in left.max(used.min_column)..=right {
                let input = self.model.get_cell_content(sheet, row, col)?;
                let display = self.model.get_formatted_cell_value(sheet, row, col)?;
                let style = self.cell_style(&self.model.get_cell_style(sheet, row, col)?);
                if input.is_empty() && display.is_empty() && style.is_none() {
                    continue;
                }
                let kind = match self.model.get_cell_type(sheet, row, col)? {
                    CellType::Number => CellKind::Number,
                    CellType::ErrorValue => CellKind::Error,
                    _ => CellKind::Text,
                };
                view.push(CellView { col: (col - 1) as u32, row: (row - 1) as u32, input, display, kind, style });
            }
        }
        Ok(view)
    }

    /// Reduz o estilo do IronCalc ao que a grade desenha. `None` se for o padrão.
    fn cell_style(&self, style: &Style) -> Option<CellStyle> {
        let color = |color| Some(self.model.resolve_color(color)).filter(|hex| !hex.is_empty());
        let align = match style.alignment.as_ref().map(|a| &a.horizontal) {
            Some(HorizontalAlignment::Left) => Some(Align::Left),
            Some(HorizontalAlignment::Center | HorizontalAlignment::CenterContinuous) => Some(Align::Center),
            Some(HorizontalAlignment::Right) => Some(Align::Right),
            _ => None,
        };
        let cell_style = CellStyle {
            bold: style.font.b,
            italic: style.font.i,
            underline: style.font.u,
            strike: style.font.strike,
            color: color(&style.font.color),
            fill: color(&style.fill.color),
            align,
        };
        (cell_style != CellStyle::default()).then_some(cell_style)
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
    use ironcalc::base::expressions::types::Area;

    use super::*;

    /// Preenche células da primeira planilha, dadas como (coluna, linha,
    /// texto) a partir de zero.
    fn engine(cells: &[(u32, u32, &str)]) -> Engine {
        let mut engine = Engine::new().unwrap();
        for (col, row, input) in cells {
            engine.set_cell(0, *col, *row, input).unwrap();
        }
        engine
    }

    fn cell_in(engine: &Engine, sheet: u32, col: u32, row: u32) -> Option<CellView> {
        engine.view(sheet, col, row, 1, 1).unwrap().pop()
    }

    fn cell(engine: &Engine, col: u32, row: u32) -> Option<CellView> {
        cell_in(engine, 0, col, row)
    }

    fn shown(engine: &Engine, col: u32, row: u32) -> String {
        cell(engine, col, row).map(|c| c.display).unwrap_or_default()
    }

    /// Aplica um estilo a uma célula da primeira planilha.
    fn set_style(engine: &mut Engine, col: u32, row: u32, path: &str, value: &str) {
        let area = Area { sheet: 0, row: row as i32 + 1, column: col as i32 + 1, width: 1, height: 1 };
        engine.model.update_range_style(&area, path, value).unwrap();
    }

    /// Caminho de arquivo temporário exclusivo deste teste.
    fn temp_xlsx(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("abaco-{}-{name}.xlsx", std::process::id()))
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
        assert_eq!(text.style, None);
        let formula = cell(&e, 2, 0).unwrap();
        assert_eq!((formula.input.as_str(), formula.display.as_str(), &formula.kind), ("=B1*2", "3", &CellKind::Number));
        let error = cell(&e, 3, 0).unwrap();
        assert_eq!((error.display.as_str(), &error.kind), ("#DIV/0!", &CellKind::Error));
    }

    #[test]
    fn view_returns_only_cells_inside_the_window() {
        let e = engine(&[(0, 0, "1"), (5, 5, "2"), (20, 40, "3")]);
        let positions = |view: Vec<CellView>| view.into_iter().map(|c| (c.col, c.row)).collect::<Vec<_>>();
        assert_eq!(positions(e.view(0, 0, 0, 10, 10).unwrap()), [(0, 0), (5, 5)]);
        assert_eq!(positions(e.view(0, 5, 5, 30, 40).unwrap()), [(5, 5), (20, 40)]);
        assert_eq!(positions(e.view(0, 6, 6, 5, 5).unwrap()), []);
        assert_eq!(positions(e.view(0, 100, 100, 10, 10).unwrap()), []);
    }

    #[test]
    fn empty_input_clears_the_cell() {
        let mut e = engine(&[(0, 0, "10"), (1, 0, "=A1*2")]);
        assert_eq!(shown(&e, 1, 0), "20");
        e.set_cell(0, 0, 0, "   ").unwrap();
        assert!(cell(&e, 0, 0).is_none());
        assert_eq!(shown(&e, 1, 0), "0");
    }

    #[test]
    fn undo_and_redo() {
        let mut e = engine(&[(0, 0, "10")]);
        e.set_cell(0, 0, 0, "25").unwrap();
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
        assert!(e.set_cell(0, 16_384, 0, "1").is_err());
        assert!(e.set_cell(0, 0, 1_048_576, "1").is_err());
        assert!(e.set_cell(0, 16_383, 1_048_575, "1").is_ok());
        assert!(e.set_cell(7, 0, 0, "1").is_err());
        assert!(e.view(7, 0, 0, 10, 10).is_err());
    }

    #[test]
    fn sheets_are_independent_and_can_reference_each_other() {
        let mut e = engine(&[(0, 0, "10")]);
        assert_eq!(e.sheets().len(), 1);

        let second = e.add_sheet().unwrap();
        assert_eq!(second, 1);
        let names: Vec<String> = e.sheets().into_iter().map(|s| s.name).collect();
        assert_eq!(names.len(), 2);

        assert!(cell_in(&e, second, 0, 0).is_none());
        e.set_cell(second, 0, 0, &format!("={}!A1*3", names[0])).unwrap();
        assert_eq!(cell_in(&e, second, 0, 0).unwrap().display, "30");
        // A primeira planilha não mudou.
        assert_eq!(shown(&e, 0, 0), "10");
    }

    #[test]
    fn view_reports_styles() {
        let mut e = engine(&[(0, 0, "Total"), (1, 0, "5")]);
        set_style(&mut e, 0, 0, "font.b", "true");
        set_style(&mut e, 0, 0, "font.color", "#0F6B30");
        set_style(&mut e, 0, 0, "fill.color", "#E3F4E6");
        set_style(&mut e, 0, 0, "alignment.horizontal", "center");
        // Célula vazia, só com cor de fundo.
        set_style(&mut e, 3, 3, "fill.color", "#E7E5FB");

        let total = cell(&e, 0, 0).unwrap().style.unwrap();
        assert!(total.bold && !total.italic);
        assert_eq!(total.color.as_deref(), Some("#0F6B30"));
        assert_eq!(total.fill.as_deref(), Some("#E3F4E6"));
        assert_eq!(total.align, Some(Align::Center));

        assert_eq!(cell(&e, 1, 0).unwrap().style, None);

        let filled = cell(&e, 3, 3).unwrap();
        assert_eq!((filled.input.as_str(), filled.display.as_str()), ("", ""));
        assert_eq!(filled.style.unwrap().fill.as_deref(), Some("#E7E5FB"));
    }

    #[test]
    fn saves_and_reopens_xlsx() {
        let mut e = engine(&[(0, 0, "Janeiro"), (1, 0, "18.500,00"), (2, 0, "9200"), (3, 0, "=B1-C1")]);
        set_style(&mut e, 0, 0, "font.b", "true");
        // Célula vazia, só com cor de fundo.
        set_style(&mut e, 4, 0, "fill.color", "#E7E5FB");
        let second = e.add_sheet().unwrap();
        e.set_cell(second, 0, 0, "outra planilha").unwrap();

        let path = temp_xlsx("roundtrip");
        e.save(&path).unwrap();
        // Salvar por cima de um arquivo existente também funciona.
        e.save(&path).unwrap();

        let reopened = Engine::open(&path).unwrap();
        fs::remove_file(&path).unwrap();

        assert_eq!(reopened.sheets(), e.sheets());
        let formula = cell(&reopened, 3, 0).unwrap();
        assert_eq!((formula.input.as_str(), formula.display.as_str()), ("=B1-C1", "9.300,00"));
        assert_eq!(shown(&reopened, 1, 0), "18.500,00");
        assert!(cell(&reopened, 0, 0).unwrap().style.unwrap().bold);
        let filled = cell(&reopened, 4, 0).unwrap();
        assert_eq!((filled.input.as_str(), filled.display.as_str()), ("", ""));
        assert_eq!(filled.style.unwrap().fill.as_deref(), Some("#E7E5FB"));
        assert_eq!(cell_in(&reopened, second, 0, 0).unwrap().display, "outra planilha");
    }

    #[test]
    fn open_reports_missing_or_invalid_files() {
        assert!(Engine::open(&temp_xlsx("nao-existe")).is_err());

        let path = temp_xlsx("invalido");
        fs::write(&path, "isto não é um xlsx").unwrap();
        let result = Engine::open(&path);
        fs::remove_file(&path).unwrap();
        assert!(result.is_err());
    }
}
