//! Tabelas dinâmicas na planilha: lê a origem, escreve a grade calculada e a
//! estiliza, e guarda as definições no `.xlsx` (o Excel as vê como valores).

use ironcalc::base::cell::CellValue;

use crate::engine::{Engine, Range, StyleProperty, area, to_ironcalc};
use crate::package;
use crate::pivot::{self, Field, Grid, Out, Pivot, Role, Source, SrcCell};
use crate::validation::parse_range_text;

/// Passa disto, a grade não é escrita.
const MAX_GRID_CELLS: usize = 1_000_000;
const HEADER_FILL: &str = "#EEF2FF";
const GRAND_FILL: &str = "#E0E7FF";
/// Parte do pacote onde ficam as definições.
const PART: &str = "docProps/abacus-pivots.xml";

/// Número como o campo de entrada o espera (vírgula decimal).
fn input_number(n: f64) -> String {
    let text = if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else if n.abs() >= 1e-6 && n.abs() < 1e15 {
        format!("{n}")
    } else {
        format!("{n:.10}").trim_end_matches('0').trim_end_matches('.').to_string()
    };
    text.replace('.', ",")
}

/// Um campo do texto separado por tabulações, com aspas quando precisa.
fn tsv_field(text: &str) -> String {
    let text = if text.starts_with('=') { format!(" {text}") } else { text.to_string() };
    if text.contains(['\t', '"', '\n', '\r']) { format!("\"{}\"", text.replace('"', "\"\"")) } else { text }
}

fn grid_text(grid: &Grid) -> String {
    grid.cells
        .iter()
        .map(|row| {
            row.iter()
                .map(|cell| match cell {
                    Out::Empty => String::new(),
                    Out::Text(text) => tsv_field(text),
                    Out::Num(n) => input_number(*n),
                })
                .collect::<Vec<_>>()
                .join("\t")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn overlaps(a: Range, b: Range) -> bool {
    a.col < b.col + b.cols && b.col < a.col + a.cols && a.row < b.row + b.rows && b.row < a.row + a.rows
}

impl Engine {
    fn pivot_extent(pivot: &Pivot) -> Option<Range> {
        (pivot.width > 0 && pivot.height > 0).then_some(Range {
            col: pivot.col,
            row: pivot.row,
            cols: pivot.width,
            rows: pivot.height,
        })
    }

    /// Lê os dados de origem: a primeira linha dá os nomes dos campos.
    fn read_source(&self, sheet: u32, text: &str) -> Result<Source, String> {
        let range = parse_range_text(text).ok_or("escreva as células de origem assim: A1:E100")?;
        if usize::try_from(sheet).map_or(true, |s| s >= self.sheet_names().len()) {
            return Err("a planilha de origem não existe".into());
        }
        if range.rows < 1 || range.cols < 1 {
            return Err("a origem está vazia".into());
        }
        let read = |col: u32, row: u32| -> SrcCell {
            let Ok((r, c)) = to_ironcalc(col, row) else { return SrcCell::default() };
            let num = match self.model.get_model().get_cell_value_by_index(sheet, r, c) {
                Ok(CellValue::Number(n)) => Some(n),
                _ => None,
            };
            SrcCell { text: self.model.get_formatted_cell_value(sheet, r, c).unwrap_or_default(), num }
        };
        let mut headers: Vec<String> = Vec::new();
        for col in range.col..range.col + range.cols {
            let mut name = read(col, range.row).text.trim().to_string();
            if name.is_empty() {
                name = format!("Coluna {}", crate::charts::col_letters(col));
            }
            let base = name.clone();
            let mut n = 2;
            while headers.contains(&name) {
                name = format!("{base} {n}");
                n += 1;
            }
            headers.push(name);
        }
        let rows = (range.row + 1..range.row + range.rows)
            .map(|row| (range.col..range.col + range.cols).map(|col| read(col, row)).collect::<Vec<_>>())
            .filter(|record| record.iter().any(|c| !c.text.is_empty() || c.num.is_some()))
            .collect();
        Ok(Source { headers, rows })
    }

    /// Nomes dos campos de uma origem.
    pub fn pivot_fields(&self, sheet: u32, source: &str) -> Result<Vec<String>, String> {
        Ok(self.read_source(sheet, source)?.headers)
    }

    /// Itens de um campo (já agrupado), para a lista de filtro.
    pub fn pivot_items(&self, sheet: u32, source: &str, field: Field) -> Result<Vec<String>, String> {
        let data = self.read_source(sheet, source)?;
        pivot::distinct_items(&field, &data)
    }

    /// Tabelas dinâmicas cuja grade está na planilha.
    pub fn pivots(&self, sheet: u32) -> Vec<Pivot> {
        self.pivots.iter().filter(|p| p.sheet == sheet).cloned().collect()
    }

    /// Escreve a grade da tabela e guarda a definição.
    fn render_pivot(&mut self, mut pivot: Pivot, is_new: bool) -> Result<Pivot, String> {
        let source = self.read_source(pivot.source_sheet, &pivot.source)?;
        let grid = pivot::compute(&pivot, &source)?;
        let (height, width) = (grid.cells.len(), grid.cells.first().map_or(0, Vec::len));
        if height * width > MAX_GRID_CELLS {
            return Err("a tabela ficou grande demais; use menos campos ou filtre".into());
        }
        if usize::try_from(pivot.sheet).map_or(true, |s| s >= self.sheet_names().len()) {
            return Err("a planilha da tabela não existe".into());
        }
        let at = Range { col: pivot.col, row: pivot.row, cols: width as u32, rows: height as u32 };
        area(pivot.sheet, at).map_err(|_| "a tabela não cabe a partir dessa célula".to_string())?;
        let origin = parse_range_text(&pivot.source).ok_or("origem inválida")?;
        if pivot.sheet == pivot.source_sheet && overlaps(at, origin) {
            return Err("a tabela dinâmica não pode ficar sobre os dados de origem".into());
        }
        if self.pivots.iter().any(|p| {
            p.id != pivot.id && p.sheet == pivot.sheet && Self::pivot_extent(p).is_some_and(|e| overlaps(e, at))
        }) {
            return Err("essa área já é de outra tabela dinâmica".into());
        }
        let previous = Self::pivot_extent(&pivot).filter(|_| !is_new);
        // Não escreve por cima de dados que não são desta tabela.
        let (sheet_index, dim) = (pivot.sheet, self.model.get_model().workbook.worksheet(pivot.sheet)?.dimension());
        let first_new_row = at.row;
        for row in at.row..at.row + at.rows {
            if row + 1 > dim.max_row as u32 {
                break;
            }
            for col in at.col..at.col + at.cols {
                if previous.is_some_and(|e| overlaps(e, Range { col, row, cols: 1, rows: 1 })) {
                    continue;
                }
                let (r, c) = to_ironcalc(col, row)?;
                if !self.model.get_cell_content(sheet_index, r, c)?.is_empty() {
                    return Err(format!(
                        "a célula {}{} já tem dados; escolha uma área vazia para a tabela",
                        crate::charts::col_letters(col),
                        row + 1
                    ));
                }
            }
        }
        let _ = first_new_row;
        if let Some(old) = previous {
            self.model.range_clear_all(&area(pivot.sheet, old)?)?;
        }
        let target = area(pivot.sheet, at)?;
        // O IronCalc cola a partir da seleção e precisa dela dentro do que cola.
        self.select(pivot.sheet, target.row, target.column, target.row, target.column)?;
        self.model.paste_csv_string(
            &ironcalc::base::expressions::types::Area { width: 1, height: 1, ..target },
            &grid_text(&grid),
        )?;
        self.style_pivot(pivot.sheet, at, &grid)?;
        pivot.width = width as u32;
        pivot.height = height as u32;
        // Deixa a seleção na tabela.
        let (r, c) = to_ironcalc(at.col, at.row)?;
        self.select(pivot.sheet, r, c, r, c)?;
        Ok(pivot)
    }

    /// Alarga as colunas que não comportam o texto, sem nunca estreitar.
    fn fit_pivot_columns(&mut self, sheet: u32, at: Range, grid: &Grid) -> Result<(), String> {
        for c in 0..at.cols as usize {
            let chars = grid
                .cells
                .iter()
                .map(|row| match &row[c] {
                    Out::Empty => 0,
                    Out::Text(text) => text.chars().count(),
                    // Dígitos inteiros, separadores de milhar e duas casas decimais.
                    Out::Num(n) => {
                        let digits = (n.abs().trunc() as u64).to_string().len();
                        digits + digits / 3 + 3 + usize::from(*n < 0.0)
                    }
                })
                .max()
                .unwrap_or(0);
            let width = (chars as f64 * 7.5 + 22.0).clamp(90.0, 320.0);
            let column = at.col + c as u32;
            if width > 90.0 {
                self.set_col_width(sheet, column, width)?;
            }
        }
        Ok(())
    }

    fn style_pivot(&mut self, sheet: u32, at: Range, grid: &Grid) -> Result<(), String> {
        self.fit_pivot_columns(sheet, at, grid)?;
        let cols = at.cols;
        let rows_of =
            |first: usize, count: usize| Range { col: at.col, row: at.row + first as u32, cols, rows: count as u32 };
        let header = rows_of(0, grid.header_rows);
        self.format(sheet, header, StyleProperty::Bold, "true")?;
        self.format(sheet, header, StyleProperty::Fill, HEADER_FILL)?;
        let body_rows = at.rows as usize - grid.header_rows;
        for (i, role) in grid.row_roles.iter().enumerate().skip(grid.header_rows) {
            let line = rows_of(i, 1);
            match role {
                Role::Subtotal => self.format(sheet, line, StyleProperty::Bold, "true")?,
                Role::Grand => {
                    self.format(sheet, line, StyleProperty::Bold, "true")?;
                    self.format(sheet, line, StyleProperty::Fill, GRAND_FILL)?;
                }
                _ => {}
            }
        }
        if body_rows > 0 {
            for (c, role) in grid.col_roles.iter().enumerate().skip(grid.label_cols) {
                let column = Range {
                    col: at.col + c as u32,
                    row: at.row + grid.header_rows as u32,
                    cols: 1,
                    rows: body_rows as u32,
                };
                if let Some(format) = &grid.col_formats[c] {
                    self.format(sheet, column, StyleProperty::Format, format)?;
                }
                match role {
                    Role::Subtotal => self.format(sheet, column, StyleProperty::Bold, "true")?,
                    Role::Grand => {
                        self.format(sheet, column, StyleProperty::Bold, "true")?;
                        self.format(sheet, column, StyleProperty::Fill, GRAND_FILL)?;
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    pub fn add_pivot(&mut self, pivot: Pivot) -> Result<Pivot, String> {
        let mut pivot = Pivot { id: 0, width: 0, height: 0, ..pivot };
        pivot.id = self.pivots.iter().map(|p| p.id).max().unwrap_or(0) + 1;
        let done = self.render_pivot(pivot, true)?;
        self.pivots.push(done.clone());
        Ok(done)
    }

    /// Troca a definição e redesenha a tabela.
    pub fn update_pivot(&mut self, pivot: Pivot) -> Result<Pivot, String> {
        let old = self.pivots.iter().find(|p| p.id == pivot.id).ok_or("a tabela dinâmica não existe mais")?.clone();
        let pivot = Pivot { width: old.width, height: old.height, ..pivot };
        let done = self.render_pivot(pivot, false)?;
        let slot = self.pivots.iter_mut().find(|p| p.id == done.id).ok_or("a tabela dinâmica não existe mais")?;
        *slot = done.clone();
        Ok(done)
    }

    /// Lê a origem de novo e redesenha.
    pub fn refresh_pivot(&mut self, id: u32) -> Result<Pivot, String> {
        let pivot = self.pivots.iter().find(|p| p.id == id).ok_or("a tabela dinâmica não existe mais")?.clone();
        self.update_pivot(pivot)
    }

    /// Refaz as tabelas cujos dados de origem incluem a célula. Uma falha não
    /// atrapalha a edição: a tabela fica como estava até a próxima atualização.
    pub(crate) fn refresh_pivots_fed_by(&mut self, sheet: u32, col: u32, row: u32) {
        let cell = Range { col, row, cols: 1, rows: 1 };
        let ids: Vec<u32> = self
            .pivots
            .iter()
            .filter(|p| {
                p.source_sheet == sheet && parse_range_text(&p.source).is_some_and(|source| overlaps(source, cell))
            })
            .map(|p| p.id)
            .collect();
        for id in ids {
            let _ = self.refresh_pivot(id);
        }
    }

    /// Redesenha todas as tabelas (por exemplo, depois de mudar os dados).
    pub fn refresh_all_pivots(&mut self) -> Result<u32, String> {
        let ids: Vec<u32> = self.pivots.iter().map(|p| p.id).collect();
        for id in &ids {
            self.refresh_pivot(*id)?;
        }
        Ok(ids.len() as u32)
    }

    /// Apaga a definição; com `clear`, também a grade.
    pub fn delete_pivot(&mut self, id: u32, clear: bool) -> Result<(), String> {
        let at = self.pivots.iter().position(|p| p.id == id).ok_or("a tabela dinâmica não existe mais")?;
        let pivot = self.pivots.remove(at);
        if clear && let Some(extent) = Self::pivot_extent(&pivot) {
            self.model.range_clear_all(&area(pivot.sheet, extent)?)?;
        }
        Ok(())
    }

    /// Tabelas de uma planilha excluída somem; as das seguintes sobem um índice.
    pub(crate) fn drop_pivots_of(&mut self, sheet: u32) {
        self.pivots.retain(|p| p.sheet != sheet && p.source_sheet != sheet);
        for p in &mut self.pivots {
            if p.sheet > sheet {
                p.sheet -= 1;
            }
            if p.source_sheet > sheet {
                p.source_sheet -= 1;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Arquivo
// ---------------------------------------------------------------------------

/// Guarda as definições numa parte do pacote que o Excel ignora.
pub fn write_into(xlsx: Vec<u8>, pivots: &[Pivot]) -> Result<Vec<u8>, String> {
    if pivots.is_empty() {
        return Ok(xlsx);
    }
    let mut parts = package::read_parts(&xlsx)?;
    let json = serde_json::to_string(pivots).map_err(|e| e.to_string())?;
    // Dentro de CDATA; `]]>` não aparece num JSON de texto comum, mas se aparecer é cortado.
    let body = json.replace("]]>", "]] >");
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<abacusPivots><![CDATA[{body}]]></abacusPivots>"
    );
    parts.retain(|(name, _)| name != PART);
    parts.push((PART.to_string(), xml.into_bytes()));
    package::write_parts(parts)
}

/// As definições que o arquivo traz, com ids a partir de 1.
pub fn read_from(xlsx: &[u8]) -> Vec<Pivot> {
    let Ok(parts) = package::read_parts(xlsx) else { return Vec::new() };
    let Some((_, bytes)) = parts.iter().find(|(name, _)| name == PART) else { return Vec::new() };
    let text = String::from_utf8_lossy(bytes);
    let (Some(start), Some(end)) = (text.find("<![CDATA["), text.rfind("]]>")) else { return Vec::new() };
    let mut pivots: Vec<Pivot> = serde_json::from_str(&text[start + 9..end]).unwrap_or_default();
    for (i, p) in pivots.iter_mut().enumerate() {
        p.id = i as u32 + 1;
    }
    pivots
}
