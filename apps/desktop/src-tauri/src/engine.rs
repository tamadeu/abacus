//! Ponte entre o Ábaco e o motor de planilhas [IronCalc](https://www.ironcalc.com).
//!
//! A interface usa planilhas, colunas e linhas contadas a partir de zero; o
//! IronCalc conta linhas e colunas a partir de um. A conversão acontece só
//! aqui.

use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use ironcalc::base::UserModel;
use ironcalc::base::ClipboardData;
use ironcalc::base::expressions::types::Area;
use ironcalc::base::types::{BorderItem, BorderStyle, CellType, Color, HorizontalAlignment, Style};
use ironcalc::base::BorderArea;
use ironcalc::export::save_xlsx_to_writer;
use ironcalc::import::load_from_xlsx;
use serde::{Deserialize, Serialize};

/// Localidade dos números, datas e moeda: português do Brasil. Vem do fork
/// do IronCalc declarado no `Cargo.toml`; o IronCalc 0.9 não a tem.
const LOCALE: &str = "pt";
/// Idioma dos nomes de função e dos erros (`SOMA`, `#NOME?`), do mesmo fork.
const LANGUAGE: &str = "pt";
const TIMEZONE: &str = "America/Sao_Paulo";
/// Limites de quantas colunas e linhas uma única tela pode pedir, para o
/// caso de muitas delas estarem ocultas (largura ou altura zero).
const MAX_VIEW_COLS: usize = 500;
const MAX_VIEW_ROWS: usize = 2000;

/// O pedaço de uma planilha que cabe na tela.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    /// Largura, em pixels, de cada coluna visível a partir da primeira.
    pub col_widths: Vec<f64>,
    /// Altura, em pixels, de cada linha visível a partir da primeira.
    pub row_heights: Vec<f64>,
    /// Células com conteúdo ou estilo, já calculadas. Inclui a célula
    /// principal de cada área mesclada da tela, mesmo que ela esteja fora.
    pub cells: Vec<CellView>,
    /// Áreas mescladas que aparecem na tela.
    pub merges: Vec<Range>,
}

/// Um retângulo de células, com coluna e linha iniciais a partir de zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Range {
    pub col: u32,
    pub row: u32,
    pub cols: u32,
    pub rows: u32,
}

/// Quais bordas de um intervalo desenhar.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BorderKind {
    /// Todas as linhas, por dentro e por fora.
    All,
    /// Só o contorno do intervalo.
    Outer,
    Top,
    Bottom,
    Left,
    Right,
    /// Remove as bordas.
    None,
}

/// Inserção ou exclusão de linhas ou colunas inteiras.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Structure {
    InsertRows,
    DeleteRows,
    InsertCols,
    DeleteCols,
}

/// Direção de um preenchimento com a alça da seleção.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FillAxis {
    /// Para baixo ou para cima.
    Rows,
    /// Para a direita ou para a esquerda.
    Cols,
}

/// Propriedades de estilo que a interface pode alterar.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StyleProperty {
    /// `"true"` ou `"false"`.
    Bold,
    Italic,
    Underline,
    Strike,
    /// Cor do texto, como `#RRGGBB`; vazio volta à cor padrão.
    Color,
    /// Cor de fundo, como `#RRGGBB`; vazio remove o fundo.
    Fill,
    /// `"left"`, `"center"`, `"right"` ou `"general"`.
    Align,
    /// Formato numérico do Excel, como `0.00%`.
    Format,
    /// Quebra de texto: `"true"` ou `"false"`.
    Wrap,
}

impl StyleProperty {
    /// Caminho da propriedade no estilo do IronCalc.
    fn path(self) -> &'static str {
        match self {
            StyleProperty::Bold => "font.b",
            StyleProperty::Italic => "font.i",
            StyleProperty::Underline => "font.u",
            StyleProperty::Strike => "font.strike",
            StyleProperty::Color => "font.color",
            StyleProperty::Fill => "fill.color",
            StyleProperty::Align => "alignment.horizontal",
            StyleProperty::Format => "num_fmt",
            StyleProperty::Wrap => "alignment.wrap_text",
        }
    }
}

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
    /// O texto quebra em várias linhas dentro da célula.
    pub wrap: bool,
    /// Bordas da célula, na ordem: cima, direita, baixo, esquerda.
    pub borders: [Option<BorderLine>; 4],
}

/// Uma borda de célula, reduzida ao que a grade desenha.
#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct BorderLine {
    /// Cor, como `#RRGGBB`.
    pub color: String,
    /// Espessura em pixels: 1 (fina), 2 (média) ou 3 (grossa ou dupla).
    pub width: u8,
    /// Tracejada ou pontilhada.
    pub dashed: bool,
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

/// O que foi copiado ou recortado por último dentro do Ábaco.
struct Copied {
    sheet: u32,
    /// Intervalo de origem, no formato do IronCalc: linhas e colunas a
    /// partir de um, (primeira linha, primeira coluna, última linha, última coluna).
    range: (i32, i32, i32, i32),
    /// Conteúdo e estilo de cada célula.
    data: ClipboardData,
    /// O mesmo conteúdo como texto separado por tabulações, que é o que vai
    /// para a área de transferência do sistema.
    text: String,
    /// Veio de um recortar: colar apaga a origem.
    cut: bool,
}

/// A pasta de trabalho aberta.
pub struct Engine {
    model: UserModel<'static>,
    copied: Option<Copied>,
}

impl Engine {
    /// Pasta nova, com uma planilha vazia.
    pub fn new() -> Result<Self, String> {
        let model = UserModel::new_empty("Pasta1", LOCALE, TIMEZONE, LANGUAGE)?;
        Ok(Self { model, copied: None })
    }

    /// Abre um arquivo `.xlsx`.
    pub fn open(path: &Path) -> Result<Self, String> {
        let file_name = path.to_str().ok_or("o caminho do arquivo não é um texto válido")?;
        let model = load_from_xlsx(file_name, LOCALE, TIMEZONE, LANGUAGE).map_err(|e| e.to_string())?;
        let mut model = UserModel::from_model(model);
        model.evaluate();
        Ok(Self { model, copied: None })
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

    /// Muda o nome de uma planilha.
    pub fn rename_sheet(&mut self, sheet: u32, name: &str) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("o nome da planilha não pode ficar vazio".into());
        }
        self.model.rename_sheet(sheet, name)
    }

    /// Exclui uma planilha. A pasta precisa ficar com pelo menos uma visível.
    pub fn delete_sheet(&mut self, sheet: u32) -> Result<(), String> {
        let visible = self.sheets();
        if visible.len() == 1 && visible[0].index == sheet {
            return Err("a pasta de trabalho precisa ter pelo menos uma planilha".into());
        }
        // O que foi copiado guarda o índice da planilha de origem, que muda.
        self.copied = None;
        self.model.delete_sheet(sheet)
    }

    /// Define a largura de uma coluna, em pixels.
    pub fn set_col_width(&mut self, sheet: u32, col: u32, width: f64) -> Result<(), String> {
        let (_, col) = to_ironcalc(col, 0)?;
        self.model.set_columns_width(sheet, col, col, check_size(width)?)
    }

    /// Define a altura de uma linha, em pixels.
    pub fn set_row_height(&mut self, sheet: u32, row: u32, height: f64) -> Result<(), String> {
        let (row, _) = to_ironcalc(0, row)?;
        self.model.set_rows_height(sheet, row, row, check_size(height)?)
    }

    /// Grava o conteúdo de uma célula. Texto vazio apaga a célula.
    pub fn set_cell(&mut self, sheet: u32, col: u32, row: u32, input: &str) -> Result<(), String> {
        let (row, col) = to_ironcalc(col, row)?;
        let input = if input.trim().is_empty() { "" } else { input };
        self.model.set_user_input(sheet, row, col, input)
    }

    /// O que cabe numa tela de `width` por `height` pixels a partir da
    /// célula dada: tamanhos de colunas e linhas e as células preenchidas.
    pub fn view(&self, sheet: u32, first_col: u32, first_row: u32, width: f64, height: f64) -> Result<View, String> {
        let (top, left) = to_ironcalc(first_col, first_row)?;
        let col_widths = sizes(left, MAX_COLS, MAX_VIEW_COLS, width, |col| self.model.get_column_width(sheet, col))?;
        let row_heights = sizes(top, MAX_ROWS, MAX_VIEW_ROWS, height, |row| self.model.get_row_height(sheet, row))?;

        // Só percorre a parte da tela que tem dados.
        let used = self.model.get_model().workbook.worksheet(sheet)?.dimension();
        let bottom = (top + row_heights.len() as i32 - 1).min(used.max_row);
        let right = (left + col_widths.len() as i32 - 1).min(used.max_column);

        let last_row = top + row_heights.len() as i32 - 1;
        let last_col = left + col_widths.len() as i32 - 1;
        let mut cells = Vec::new();
        // Áreas mescladas que cruzam a tela. A célula principal de cada uma
        // entra na lista mesmo estando fora, porque é ela que tem o conteúdo.
        let mut merges = Vec::new();
        for merge in self.model.get_merged_cells(sheet)? {
            let on_screen = merge.row <= last_row
                && merge.row + merge.height > top
                && merge.column <= last_col
                && merge.column + merge.width > left;
            if !on_screen {
                continue;
            }
            merges.push(Range {
                col: (merge.column - 1) as u32,
                row: (merge.row - 1) as u32,
                cols: merge.width as u32,
                rows: merge.height as u32,
            });
            if merge.row < top || merge.column < left {
                cells.extend(self.cell_view(sheet, merge.row, merge.column)?);
            }
        }
        for row in top.max(used.min_row)..=bottom {
            for col in left.max(used.min_column)..=right {
                cells.extend(self.cell_view(sheet, row, col)?);
            }
        }
        Ok(View { col_widths, row_heights, cells, merges })
    }

    /// O que desenhar numa célula (linha e coluna a partir de um), ou `None`
    /// se ela não tem conteúdo nem estilo.
    fn cell_view(&self, sheet: u32, row: i32, col: i32) -> Result<Option<CellView>, String> {
        let input = self.model.get_cell_content(sheet, row, col)?;
        let display = self.model.get_formatted_cell_value(sheet, row, col)?;
        let style = self.cell_style(&self.model.get_cell_style(sheet, row, col)?);
        if input.is_empty() && display.is_empty() && style.is_none() {
            return Ok(None);
        }
        let kind = match self.model.get_cell_type(sheet, row, col)? {
            CellType::Number => CellKind::Number,
            CellType::ErrorValue => CellKind::Error,
            _ => CellKind::Text,
        };
        Ok(Some(CellView { col: (col - 1) as u32, row: (row - 1) as u32, input, display, kind, style }))
    }

    /// Mescla o intervalo numa célula só, centralizada, como o "Mesclar e
    /// centralizar" do Excel. Falha se mais de uma célula tiver conteúdo.
    pub fn merge(&mut self, sheet: u32, range: Range) -> Result<(), String> {
        self.model.merge_cells_center(&area(sheet, range)?)
    }

    /// Desfaz as áreas mescladas que tocam o intervalo.
    pub fn unmerge(&mut self, sheet: u32, range: Range) -> Result<(), String> {
        self.model.unmerge_cells(&area(sheet, range)?)
    }

    /// Desenha (ou remove) bordas finas e pretas no intervalo.
    pub fn set_border(&mut self, sheet: u32, range: Range, kind: BorderKind) -> Result<(), String> {
        let kind = match kind {
            BorderKind::All => "All",
            BorderKind::Outer => "Outer",
            BorderKind::Top => "Top",
            BorderKind::Bottom => "Bottom",
            BorderKind::Left => "Left",
            BorderKind::Right => "Right",
            BorderKind::None => "None",
        };
        // Os campos de `BorderArea` são privados; o IronCalc o constrói a
        // partir de JSON também nos próprios testes.
        let border: BorderArea = serde_json::from_value(serde_json::json!({
            "item": { "style": "thin", "color": "#000000" },
            "type": kind,
        }))
        .map_err(|e| format!("borda inválida: {e}"))?;
        self.model.set_area_with_border(&area(sheet, range)?, &border)
    }

    /// Insere ou exclui `count` linhas ou colunas a partir de `index` (a
    /// partir de zero). As fórmulas que apontam para depois acompanham.
    pub fn change_structure(&mut self, sheet: u32, change: Structure, index: u32, count: u32) -> Result<(), String> {
        let index = to_i32(index)?.saturating_add(1);
        let count = to_i32(count)?;
        if count < 1 {
            return Err("a quantidade precisa ser de pelo menos um".into());
        }
        // O que foi copiado guarda posições que acabam de mudar.
        self.copied = None;
        match change {
            Structure::InsertRows => self.model.insert_rows(sheet, index, count),
            Structure::DeleteRows => self.model.delete_rows(sheet, index, count),
            Structure::InsertCols => self.model.insert_columns(sheet, index, count),
            Structure::DeleteCols => self.model.delete_columns(sheet, index, count),
        }
    }

    /// Altera uma propriedade de estilo em todas as células do retângulo.
    pub fn format(&mut self, sheet: u32, range: Range, property: StyleProperty, value: &str) -> Result<(), String> {
        self.model.update_range_style(&area(sheet, range)?, property.path(), value)
    }

    /// Apaga o conteúdo das células do retângulo, mantendo o estilo.
    pub fn clear(&mut self, sheet: u32, range: Range) -> Result<(), String> {
        self.model.range_clear_contents(&area(sheet, range)?)
    }

    /// Preenche a partir do intervalo até a linha ou coluna `to` (a partir de
    /// zero), como ao arrastar a alça da seleção: continua sequências e
    /// ajusta as referências das fórmulas.
    pub fn fill(&mut self, sheet: u32, range: Range, axis: FillAxis, to: u32) -> Result<(), String> {
        let source = area(sheet, range)?;
        let to = to_i32(to)?.saturating_add(1);
        match axis {
            FillAxis::Rows => self.model.auto_fill_rows(&source, to),
            FillAxis::Cols => self.model.auto_fill_columns(&source, to),
        }
    }

    /// Copia (ou recorta) o intervalo e devolve o conteúdo como texto separado
    /// por tabulações, para a área de transferência do sistema. Recortar só
    /// apaga a origem na hora de colar.
    pub fn copy(&mut self, sheet: u32, range: Range, cut: bool) -> Result<String, String> {
        let area = area(sheet, range)?;
        self.select(sheet, area.row, area.column, area.row + area.height - 1, area.column + area.width - 1)?;
        // Os campos do resultado são privados no IronCalc, mas ele pode ser
        // serializado; é assim que as outras interfaces do projeto o leem.
        let invalid = |e: serde_json::Error| format!("cópia inválida: {e}");
        let mut clipboard = serde_json::to_value(self.model.copy_to_clipboard()?).map_err(invalid)?;
        let text = clipboard["csv"].as_str().unwrap_or_default().to_string();
        let copied = Copied {
            sheet,
            range: serde_json::from_value(clipboard["range"].take()).map_err(invalid)?,
            data: serde_json::from_value(clipboard["data"].take()).map_err(invalid)?,
            text: text.clone(),
            cut,
        };
        self.copied = Some(copied);
        Ok(text)
    }

    /// Cola a partir da célula dada e devolve o intervalo preenchido, ou
    /// `None` se não havia o que colar.
    ///
    /// `text` é o texto que está na área de transferência do sistema. Se for
    /// o mesmo que o Ábaco copiou por último, a colagem leva fórmulas e
    /// estilos; se vier de outro programa, entra como texto separado por
    /// tabulações.
    pub fn paste(&mut self, sheet: u32, col: u32, row: u32, text: &str) -> Result<Option<Range>, String> {
        let (row, col) = to_ironcalc(col, row)?;
        self.select(sheet, row, col, row, col)?;
        match &self.copied {
            Some(copied) if same_text(&copied.text, text) => {
                self.model.paste_from_clipboard(copied.sheet, copied.range, &copied.data, copied.cut)?;
                // O que foi recortado só pode ser colado uma vez.
                if copied.cut {
                    self.copied = None;
                }
            }
            _ if text.trim().is_empty() => return Ok(None),
            _ => {
                let target = Area { sheet, row, column: col, width: 1, height: 1 };
                self.model.paste_csv_string(&target, text)?;
            }
        }
        // O IronCalc deixa selecionado o intervalo que acabou de preencher.
        let [first_row, first_col, last_row, last_col] = self.model.get_selected_view().range;
        Ok(Some(Range {
            col: (first_col - 1) as u32,
            row: (first_row - 1) as u32,
            cols: (last_col - first_col + 1) as u32,
            rows: (last_row - first_row + 1) as u32,
        }))
    }

    /// Seleciona um intervalo no IronCalc, que copia e cola sempre a seleção.
    fn select(&mut self, sheet: u32, row: i32, col: i32, last_row: i32, last_col: i32) -> Result<(), String> {
        self.model.set_selected_sheet(sheet)?;
        self.model.set_selected_cell(row, col)?;
        self.model.set_selected_range(row, col, last_row, last_col)
    }

    /// Reduz o estilo do IronCalc ao que a grade desenha. `None` se for o padrão.
    fn cell_style(&self, style: &Style) -> Option<CellStyle> {
        let color = |color: &Color| Some(self.model.resolve_color(color)).filter(|hex| !hex.is_empty());
        let align = match style.alignment.as_ref().map(|a| &a.horizontal) {
            Some(HorizontalAlignment::Left) => Some(Align::Left),
            Some(HorizontalAlignment::Center | HorizontalAlignment::CenterContinuous) => Some(Align::Center),
            Some(HorizontalAlignment::Right) => Some(Align::Right),
            _ => None,
        };
        let border = |item: &Option<BorderItem>| {
            item.as_ref().map(|item| BorderLine {
                color: color(&item.color).unwrap_or_else(|| "#000000".to_string()),
                width: match item.style {
                    BorderStyle::Thin | BorderStyle::Dotted | BorderStyle::SlantDashDot => 1,
                    BorderStyle::Medium
                    | BorderStyle::MediumDashed
                    | BorderStyle::MediumDashDot
                    | BorderStyle::MediumDashDotDot => 2,
                    BorderStyle::Thick | BorderStyle::Double => 3,
                },
                dashed: !matches!(
                    item.style,
                    BorderStyle::Thin | BorderStyle::Medium | BorderStyle::Thick | BorderStyle::Double
                ),
            })
        };
        let cell_style = CellStyle {
            bold: style.font.b,
            italic: style.font.i,
            underline: style.font.u,
            strike: style.font.strike,
            color: color(&style.font.color),
            fill: color(&style.fill.color),
            align,
            wrap: style.alignment.as_ref().is_some_and(|a| a.wrap_text),
            borders: [
                border(&style.border.top),
                border(&style.border.right),
                border(&style.border.bottom),
                border(&style.border.left),
            ],
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

/// Colunas e linhas de uma planilha, iguais às do Excel.
const MAX_COLS: i32 = 16_384;
const MAX_ROWS: i32 = 1_048_576;

/// Tamanhos de colunas (ou linhas) a partir de `first`, até cobrir `extent`
/// pixels, chegar ao fim da planilha ou atingir `limit` itens.
fn sizes(
    first: i32,
    last: i32,
    limit: usize,
    extent: f64,
    size_of: impl Fn(i32) -> Result<f64, String>,
) -> Result<Vec<f64>, String> {
    let mut sizes = Vec::new();
    let mut covered = 0.0;
    for index in first..=last {
        let size = size_of(index)?;
        sizes.push(size);
        covered += size;
        if covered >= extent || sizes.len() >= limit {
            break;
        }
    }
    Ok(sizes)
}

/// Compara dois textos de área de transferência, ignorando o tipo de quebra
/// de linha e os espaços nas pontas, que mudam de um programa para outro.
fn same_text(a: &str, b: &str) -> bool {
    let normalize = |text: &str| text.replace("\r\n", "\n").trim().to_string();
    normalize(a) == normalize(b)
}

/// Aceita larguras e alturas entre 1 e 2000 pixels.
fn check_size(size: f64) -> Result<f64, String> {
    if (1.0..=2000.0).contains(&size) { Ok(size) } else { Err(format!("tamanho inválido: {size}")) }
}

/// Converte um retângulo da interface para a área do IronCalc.
fn area(sheet: u32, range: Range) -> Result<Area, String> {
    if range.cols == 0 || range.rows == 0 {
        return Err("o intervalo está vazio".into());
    }
    let (row, column) = to_ironcalc(range.col, range.row)?;
    let (width, height) = (to_i32(range.cols)?, to_i32(range.rows)?);
    if column.saturating_add(width - 1) > MAX_COLS || row.saturating_add(height - 1) > MAX_ROWS {
        return Err("o intervalo passa do fim da planilha".into());
    }
    Ok(Area { sheet, row, column, width, height })
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
        engine.view(sheet, col, row, 1.0, 1.0).unwrap().cells.pop()
    }

    fn cell(engine: &Engine, col: u32, row: u32) -> Option<CellView> {
        cell_in(engine, 0, col, row)
    }

    fn shown(engine: &Engine, col: u32, row: u32) -> String {
        cell(engine, col, row).map(|c| c.display).unwrap_or_default()
    }

    /// Aplica um estilo a uma célula da primeira planilha.
    fn set_style(engine: &mut Engine, col: u32, row: u32, property: StyleProperty, value: &str) {
        engine.format(0, Range { col, row, cols: 1, rows: 1 }, property, value).unwrap();
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
            (1, 7, "=SOMA(B3:B6)"),
            (3, 7, "=SOMA(D3:D6)"),
        ]);
        assert_eq!(shown(&e, 3, 2), "9.300,00");
        assert_eq!(shown(&e, 3, 3), "10.600,00");
        assert_eq!(shown(&e, 1, 7), "40.900,00");
        assert_eq!(shown(&e, 3, 7), "19.900,00");
    }

    #[test]
    fn speaks_brazilian_portuguese() {
        let mut e = engine(&[
            (0, 0, "R$ 10,50"),
            (1, 0, "=A1*2"),
            (2, 0, "=SE(A1>10; \"caro\"; \"barato\")"),
            (3, 0, "=MÉDIA(A1:B1)"),
            (4, 0, "=NÃOEXISTE(1)"),
            (5, 0, "=VERDADEIRO"),
            (6, 0, "=SUM(A1:B1)"),
            (7, 0, "15/03/2026"),
        ]);
        // Moeda digitada vira número, e a fórmula herda o formato.
        let typed = cell(&e, 0, 0).unwrap();
        assert_eq!((typed.display.as_str(), &typed.kind), ("R$10,50", &CellKind::Number));
        assert_eq!(shown(&e, 1, 0), "R$21,00");
        assert_eq!(shown(&e, 2, 0), "caro");
        assert_eq!(shown(&e, 3, 0), "R$15,75");
        assert_eq!(shown(&e, 4, 0), "#NOME?");
        assert_eq!(shown(&e, 5, 0), "VERDADEIRO");
        // O nome em inglês não vale: o idioma das fórmulas é o português.
        assert_eq!(shown(&e, 6, 0), "#NOME?");
        assert_eq!(cell(&e, 7, 0).unwrap().kind, CellKind::Number);
        assert_eq!(shown(&e, 7, 0), "15/03/2026");

        assert_eq!(e.sheets()[0].name, "Planilha1");
        e.add_sheet().unwrap();
        assert_eq!(e.sheets()[1].name, "Planilha2");
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
        // Com os tamanhos padrão (90 x 25 pixels), 900 x 250 são 10 colunas e 10 linhas.
        let positions = |first_col, first_row, cols: f64, rows: f64| {
            let view = e.view(0, first_col, first_row, cols * 90.0, rows * 25.0).unwrap();
            assert_eq!((view.col_widths.len(), view.row_heights.len()), (cols as usize, rows as usize));
            view.cells.into_iter().map(|c| (c.col, c.row)).collect::<Vec<_>>()
        };
        assert_eq!(positions(0, 0, 10.0, 10.0), [(0, 0), (5, 5)]);
        assert_eq!(positions(5, 5, 30.0, 40.0), [(5, 5), (20, 40)]);
        assert_eq!(positions(6, 6, 5.0, 5.0), []);
        assert_eq!(positions(100, 100, 10.0, 10.0), []);
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
        let e = engine(&[(0, 0, "=LET(x;2;x*3)"), (1, 0, "=LAMBDA(x;x+1)(2)"), (2, 0, "=SEQUÊNCIA(3)")]);
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
        assert!(e.view(7, 0, 0, 900.0, 250.0).is_err());
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
        set_style(&mut e, 0, 0, StyleProperty::Bold, "true");
        set_style(&mut e, 0, 0, StyleProperty::Color, "#0F6B30");
        set_style(&mut e, 0, 0, StyleProperty::Fill, "#E3F4E6");
        set_style(&mut e, 0, 0, StyleProperty::Align, "center");
        // Célula vazia, só com cor de fundo.
        set_style(&mut e, 3, 3, StyleProperty::Fill, "#E7E5FB");

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
    fn view_reports_column_and_row_sizes() {
        let mut e = Engine::new().unwrap();
        e.model.set_columns_width(0, 2, 2, 200.0).unwrap();
        e.model.set_rows_height(0, 1, 1, 60.0).unwrap();

        let view = e.view(0, 0, 0, 300.0, 100.0).unwrap();
        // 90 + 200 + 90 cobrem 300 pixels; 60 + 25 + 25 cobrem 100.
        assert_eq!(view.col_widths, [90.0, 200.0, 90.0]);
        assert_eq!(view.row_heights, [60.0, 25.0, 25.0]);

        // A última coluna e a última linha da planilha encerram a tela.
        let end = e.view(0, 16_383, 1_048_575, 300.0, 100.0).unwrap();
        assert_eq!((end.col_widths.len(), end.row_heights.len()), (1, 1));
    }

    #[test]
    fn resizes_columns_and_rows() {
        let mut e = Engine::new().unwrap();
        e.set_col_width(0, 1, 160.0).unwrap();
        e.set_row_height(0, 2, 40.0).unwrap();
        let view = e.view(0, 0, 0, 300.0, 80.0).unwrap();
        assert_eq!(view.col_widths, [90.0, 160.0, 90.0]);
        assert_eq!(view.row_heights, [25.0, 25.0, 40.0]);

        e.undo().unwrap();
        assert_eq!(e.view(0, 0, 0, 300.0, 80.0).unwrap().row_heights, [25.0, 25.0, 25.0, 25.0]);

        for bad in [0.0, -5.0, 5000.0, f64::NAN] {
            assert!(e.set_col_width(0, 0, bad).is_err(), "{bad}");
            assert!(e.set_row_height(0, 0, bad).is_err(), "{bad}");
        }
        assert!(e.set_col_width(0, 16_384, 100.0).is_err());
    }

    #[test]
    fn renames_and_deletes_sheets() {
        let mut e = engine(&[(0, 0, "10")]);
        let second = e.add_sheet().unwrap();
        e.rename_sheet(0, "  Receitas ").unwrap();
        e.set_cell(second, 0, 0, "=Receitas!A1*2").unwrap();
        assert_eq!(e.sheets()[0].name, "Receitas");
        assert_eq!(cell_in(&e, second, 0, 0).unwrap().display, "20");

        assert!(e.rename_sheet(0, "   ").is_err());
        let other = e.sheets()[1].name.clone();
        assert!(e.rename_sheet(0, &other).is_err(), "nome repetido");
        assert!(e.rename_sheet(9, "X").is_err());

        e.delete_sheet(second).unwrap();
        assert_eq!(e.sheets().len(), 1);
        assert!(e.delete_sheet(0).is_err(), "não exclui a última planilha");
        assert!(e.delete_sheet(9).is_err());

        e.undo().unwrap();
        assert_eq!(e.sheets().len(), 2);
    }

    #[test]
    fn formats_a_range() {
        let mut e = engine(&[(0, 0, "0,5"), (1, 0, "1234,5"), (0, 1, "texto")]);
        let range = Range { col: 0, row: 0, cols: 2, rows: 2 };
        e.format(0, range, StyleProperty::Bold, "true").unwrap();
        e.format(0, range, StyleProperty::Fill, "#E7E5FB").unwrap();
        for (col, row) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            let style = cell(&e, col, row).unwrap().style.unwrap();
            assert!(style.bold, "{col},{row}");
            assert_eq!(style.fill.as_deref(), Some("#E7E5FB"));
        }
        // Fora do intervalo nada muda.
        assert!(cell(&e, 2, 0).is_none());

        e.format(0, Range { col: 0, row: 0, cols: 1, rows: 1 }, StyleProperty::Format, "0.00%").unwrap();
        assert_eq!(shown(&e, 0, 0), "50,00%");
        e.format(0, Range { col: 1, row: 0, cols: 1, rows: 1 }, StyleProperty::Format, "\"R$\" #,##0.00").unwrap();
        assert_eq!(shown(&e, 1, 0), "R$ 1.234,50");

        // Vazio remove a cor; "false" desliga o negrito.
        e.format(0, range, StyleProperty::Fill, "").unwrap();
        e.format(0, range, StyleProperty::Bold, "false").unwrap();
        assert_eq!(cell(&e, 0, 1).unwrap().style, None);

        assert!(e.format(0, range, StyleProperty::Align, "diagonal").is_err());
        assert!(e.format(0, Range { col: 0, row: 0, cols: 0, rows: 1 }, StyleProperty::Bold, "true").is_err());
        assert!(e.format(0, Range { col: 16_383, row: 0, cols: 2, rows: 1 }, StyleProperty::Bold, "true").is_err());
    }

    #[test]
    fn clears_a_range_but_keeps_styles() {
        let mut e = engine(&[(0, 0, "1"), (1, 0, "2"), (2, 0, "3"), (0, 1, "=A1+C1")]);
        set_style(&mut e, 0, 0, StyleProperty::Bold, "true");
        e.clear(0, Range { col: 0, row: 0, cols: 2, rows: 1 }).unwrap();
        let cleared = cell(&e, 0, 0).unwrap();
        assert_eq!(cleared.input, "");
        assert!(cleared.style.unwrap().bold);
        assert!(cell(&e, 1, 0).is_none());
        assert_eq!(shown(&e, 2, 0), "3");
        assert_eq!(shown(&e, 0, 1), "3");
        e.undo().unwrap();
        assert_eq!(shown(&e, 0, 1), "4");
    }

    fn range(col: u32, row: u32, cols: u32, rows: u32) -> Range {
        Range { col, row, cols, rows }
    }

    #[test]
    fn merges_and_unmerges_cells() {
        let mut e = engine(&[(1, 1, "Título")]);
        e.merge(0, range(1, 1, 3, 2)).unwrap();

        let view = e.view(0, 0, 0, 900.0, 250.0).unwrap();
        assert_eq!(view.merges, [range(1, 1, 3, 2)]);
        let anchor = cell(&e, 1, 1).unwrap();
        assert_eq!(anchor.display, "Título");
        assert_eq!(anchor.style.unwrap().align, Some(Align::Center));

        // Com a tela começando no meio da área, a célula principal vem junto.
        let scrolled = e.view(0, 2, 2, 900.0, 250.0).unwrap();
        assert_eq!(scrolled.merges, [range(1, 1, 3, 2)]);
        assert!(scrolled.cells.iter().any(|c| (c.col, c.row) == (1, 1) && c.display == "Título"));
        // Fora da área, nada.
        assert!(e.view(0, 10, 10, 900.0, 250.0).unwrap().merges.is_empty());

        // Duas células com conteúdo não podem ser mescladas.
        e.set_cell(0, 6, 0, "a").unwrap();
        e.set_cell(0, 7, 0, "b").unwrap();
        assert!(e.merge(0, range(6, 0, 2, 1)).is_err());

        e.unmerge(0, range(2, 2, 1, 1)).unwrap();
        assert!(e.view(0, 0, 0, 900.0, 250.0).unwrap().merges.is_empty());
        assert_eq!(shown(&e, 1, 1), "Título");
        e.undo().unwrap();
        assert_eq!(e.view(0, 0, 0, 900.0, 250.0).unwrap().merges.len(), 1);
    }

    #[test]
    fn borders_and_wrap() {
        let mut e = engine(&[(0, 0, "texto bem comprido")]);
        e.set_border(0, range(0, 0, 2, 2), BorderKind::Outer).unwrap();
        let thin = |style: &CellStyle, side: usize| style.borders[side].as_ref().map(|b| (b.color.clone(), b.width, b.dashed));
        let black = Some(("#000000".to_string(), 1, false));
        // Ordem: cima, direita, baixo, esquerda.
        let top_left = cell(&e, 0, 0).unwrap().style.unwrap();
        assert_eq!([thin(&top_left, 0), thin(&top_left, 3)], [black.clone(), black.clone()]);
        assert_eq!([thin(&top_left, 1), thin(&top_left, 2)], [None, None]);
        let bottom_right = cell(&e, 1, 1).unwrap().style.unwrap();
        assert_eq!([thin(&bottom_right, 1), thin(&bottom_right, 2)], [black.clone(), black]);

        e.set_border(0, range(0, 0, 2, 2), BorderKind::None).unwrap();
        assert_eq!(cell(&e, 0, 0).unwrap().style, None);
        assert!(cell(&e, 1, 1).is_none());

        e.format(0, range(0, 0, 1, 1), StyleProperty::Wrap, "true").unwrap();
        assert!(cell(&e, 0, 0).unwrap().style.unwrap().wrap);
        e.format(0, range(0, 0, 1, 1), StyleProperty::Wrap, "false").unwrap();
        assert_eq!(cell(&e, 0, 0).unwrap().style, None);
    }

    #[test]
    fn inserts_and_deletes_rows_and_columns() {
        let mut e = engine(&[(0, 0, "1"), (0, 1, "2"), (0, 2, "=A1+A2"), (2, 0, "=A3*10")]);
        // Duas linhas novas antes da segunda: tudo desce e as fórmulas acompanham.
        e.change_structure(0, Structure::InsertRows, 1, 2).unwrap();
        assert!(cell(&e, 0, 1).is_none());
        assert_eq!(shown(&e, 0, 3), "2");
        let sum = cell(&e, 0, 4).unwrap();
        assert_eq!((sum.input.as_str(), sum.display.as_str()), ("=A1+A4", "3"));
        assert_eq!(cell(&e, 2, 0).unwrap().input, "=A5*10");

        e.change_structure(0, Structure::DeleteRows, 1, 2).unwrap();
        assert_eq!(cell(&e, 0, 2).unwrap().input, "=A1+A2");

        e.change_structure(0, Structure::InsertCols, 0, 1).unwrap();
        assert!(cell(&e, 0, 0).is_none());
        assert_eq!(cell(&e, 1, 2).unwrap().input, "=B1+B2");
        e.change_structure(0, Structure::DeleteCols, 0, 1).unwrap();
        assert_eq!(cell(&e, 0, 2).unwrap().input, "=A1+A2");

        e.undo().unwrap();
        assert_eq!(cell(&e, 1, 2).unwrap().input, "=B1+B2");
        assert!(e.change_structure(0, Structure::InsertRows, 0, 0).is_err());
    }

    #[test]
    fn fills_series_and_formulas() {
        let mut e = engine(&[(0, 0, "1"), (0, 1, "2"), (1, 0, "=A1*10"), (3, 0, "Jan")]);
        // 1, 2 arrastados até a linha 5 continuam a sequência.
        e.fill(0, range(0, 0, 1, 2), FillAxis::Rows, 4).unwrap();
        assert_eq!([shown(&e, 0, 2), shown(&e, 0, 3), shown(&e, 0, 4)], ["3", "4", "5"]);
        // A fórmula desce com a referência ajustada.
        e.fill(0, range(1, 0, 1, 1), FillAxis::Rows, 4).unwrap();
        let last = cell(&e, 1, 4).unwrap();
        assert_eq!((last.input.as_str(), last.display.as_str()), ("=A5*10", "50"));
        // Para a direita.
        e.fill(0, range(1, 0, 1, 1), FillAxis::Cols, 2).unwrap();
        assert_eq!(cell(&e, 2, 0).unwrap().input, "=B1*10");

        e.undo().unwrap();
        assert!(cell(&e, 2, 0).is_none());
        // Destino dentro do próprio intervalo não é um preenchimento.
        assert!(e.fill(0, range(0, 0, 1, 2), FillAxis::Rows, 1).is_err());
    }

    #[test]
    fn copy_and_paste_keeps_formulas_and_styles() {
        let mut e = engine(&[(0, 0, "10"), (1, 0, "20"), (2, 0, "=A1+B1"), (0, 1, "5"), (1, 1, "6")]);
        set_style(&mut e, 2, 0, StyleProperty::Bold, "true");

        let text = e.copy(0, range(2, 0, 1, 1), false).unwrap();
        assert_eq!(text, "30");
        // Colar uma linha abaixo ajusta as referências, como no Excel.
        let pasted = e.paste(0, 2, 1, &text).unwrap();
        assert_eq!(pasted, Some(range(2, 1, 1, 1)));
        let copy = cell(&e, 2, 1).unwrap();
        assert_eq!((copy.input.as_str(), copy.display.as_str()), ("=A2+B2", "11"));
        assert!(copy.style.unwrap().bold);
        // A origem continua lá e dá para colar de novo.
        assert_eq!(shown(&e, 2, 0), "30");
        assert_eq!(e.paste(0, 4, 4, &text).unwrap(), Some(range(4, 4, 1, 1)));

        // Um intervalo sai como texto separado por tabulações e quebras de linha.
        let block = e.copy(0, range(0, 0, 2, 2), false).unwrap();
        assert_eq!(block, "10\t20\n5\t6");
        assert_eq!(e.paste(0, 0, 5, &block).unwrap(), Some(range(0, 5, 2, 2)));
        assert_eq!([shown(&e, 0, 5), shown(&e, 1, 5), shown(&e, 0, 6), shown(&e, 1, 6)], ["10", "20", "5", "6"]);

        e.undo().unwrap();
        assert!(cell(&e, 0, 5).is_none());
    }

    #[test]
    fn cut_moves_the_cells_once() {
        let mut e = engine(&[(0, 0, "10"), (1, 0, "=A1*2")]);
        let text = e.copy(0, range(0, 0, 2, 1), true).unwrap();
        // Recortar não mexe em nada até colar.
        assert_eq!(shown(&e, 0, 0), "10");

        assert_eq!(e.paste(0, 0, 3, &text).unwrap(), Some(range(0, 3, 2, 1)));
        assert!(cell(&e, 0, 0).is_none());
        assert!(cell(&e, 1, 0).is_none());
        assert_eq!(shown(&e, 0, 3), "10");
        assert_eq!(shown(&e, 1, 3), "20");

        // Colar de novo o mesmo texto já não move: entra como texto de fora.
        e.paste(0, 0, 6, &text).unwrap();
        assert_eq!(shown(&e, 0, 3), "10");
        assert_eq!(shown(&e, 0, 6), "10");
    }

    #[test]
    fn pastes_text_from_other_programs() {
        let mut e = engine(&[(0, 0, "antigo")]);
        // Texto que não saiu do Ábaco, com quebras de linha do Windows.
        let pasted = e.paste(0, 1, 1, "Nome\tValor\r\nAna\t1.234,56\r\nBia\t=2+3\r\n").unwrap();
        assert_eq!(pasted, Some(range(1, 1, 2, 3)));
        assert_eq!(shown(&e, 1, 1), "Nome");
        let number = cell(&e, 2, 2).unwrap();
        assert_eq!((number.display.as_str(), &number.kind), ("1.234,56", &CellKind::Number));
        assert_eq!(shown(&e, 2, 3), "5");

        // Área de transferência vazia: nada a colar e nada muda.
        assert_eq!(e.paste(0, 0, 0, "  ").unwrap(), None);
        assert_eq!(shown(&e, 0, 0), "antigo");

        // O Ábaco copiou algo, mas depois o usuário copiou outra coisa em
        // outro programa: vale o texto de fora.
        e.copy(0, range(0, 0, 1, 1), false).unwrap();
        e.paste(0, 5, 5, "de fora").unwrap();
        assert_eq!(shown(&e, 5, 5), "de fora");
    }

    #[test]
    fn copies_between_sheets() {
        let mut e = engine(&[(0, 0, "7"), (1, 0, "=A1*3")]);
        let second = e.add_sheet().unwrap();
        let text = e.copy(0, range(0, 0, 2, 1), false).unwrap();
        assert_eq!(e.paste(second, 0, 0, &text).unwrap(), Some(range(0, 0, 2, 1)));
        let formula = cell_in(&e, second, 1, 0).unwrap();
        assert_eq!((formula.input.as_str(), formula.display.as_str()), ("=A1*3", "21"));
    }

    #[test]
    fn saves_and_reopens_xlsx() {
        let mut e = engine(&[(0, 0, "Janeiro"), (1, 0, "18.500,00"), (2, 0, "9200"), (3, 0, "=B1-C1")]);
        set_style(&mut e, 0, 0, StyleProperty::Bold, "true");
        // Célula vazia, só com cor de fundo.
        set_style(&mut e, 4, 0, StyleProperty::Fill, "#E7E5FB");
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
