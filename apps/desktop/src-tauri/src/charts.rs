//! Gráficos. O IronCalc não conhece gráficos, então o Abacus guarda os seus
//! e os grava no `.xlsx` como gráficos nativos do Excel, acrescentados ao
//! arquivo depois que o motor o salva. Na abertura, os gráficos de colunas,
//! barras, linhas e pizza do arquivo voltam a ser lidos.

use std::io::{Cursor, Read, Write};

use quick_xml::Reader;
use quick_xml::events::Event;
use serde::{Deserialize, Serialize};
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

use crate::engine::Range;

/// Colunas e linhas que um gráfico pode usar de dados, para não gastar a
/// memória à toa com um intervalo enorme.
pub const MAX_CHART_COLS: u32 = 50;
pub const MAX_CHART_ROWS: u32 = 5000;

/// Cores das séries, na ordem.
const PALETTE: [&str; 8] = ["4F46E5", "F59E0B", "10B981", "EF4444", "3B82F6", "8B5CF6", "EC4899", "14B8A6"];

const NS_CHART: &str = "http://schemas.openxmlformats.org/drawingml/2006/chart";
const NS_DRAWING: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const NS_SHEET_DRAWING: &str = "http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing";
const NS_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const REL_DRAWING: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing";
const REL_CHART: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart";
const XML_DECLARATION: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#;
/// Identificador da relação planilha → desenho; não colide com as do motor (`rId1`, …).
const DRAWING_REL_ID: &str = "rIdAbacusDrawing";

/// O tipo do gráfico.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChartKind {
    Column,
    Bar,
    Line,
    Pie,
}

/// Um gráfico: os dados vêm de um intervalo da própria planilha e ele fica
/// sobre a grade, do canto da célula `from` ao fim da célula `to` (inclusive).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Chart {
    pub id: u32,
    pub sheet: u32,
    pub kind: ChartKind,
    pub title: String,
    pub range: Range,
    pub from_col: u32,
    pub from_row: u32,
    pub to_col: u32,
    pub to_row: u32,
}

/// Uma série de valores; células vazias ou de texto ficam como `None`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Series {
    pub name: String,
    pub values: Vec<Option<f64>>,
}

/// O que o gráfico mostra, tirado das células.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChartData {
    pub categories: Vec<String>,
    pub series: Vec<Series>,
    /// A primeira coluna do intervalo é de categorias.
    #[serde(skip)]
    pub has_categories: bool,
    /// A primeira linha do intervalo é de títulos das séries.
    #[serde(skip)]
    pub has_header: bool,
}

/// O que se precisa saber de uma célula para montar o gráfico.
pub struct CellInfo {
    pub text: String,
    pub number: Option<f64>,
}

/// Como o intervalo se divide: a primeira coluna pode ser de categorias e a
/// primeira linha, de títulos das séries.
struct Layout {
    categories: bool,
    header: bool,
}

fn layout(range: Range, cell: &impl Fn(u32, u32) -> CellInfo) -> Layout {
    let first_series_col = |categories: bool| u32::from(categories);
    // A primeira coluna vira categorias quando tem texto (fora o título).
    let categories = range.cols >= 2
        && (0..range.rows).any(|offset| {
            let info = cell(range.row + offset, range.col);
            offset > 0 && info.number.is_none() && !info.text.is_empty()
        });
    // A primeira linha é de títulos quando as séries começam por texto.
    let header = range.rows >= 2
        && (first_series_col(categories)..range.cols).all(|offset| {
            let info = cell(range.row, range.col + offset);
            info.number.is_none() && !info.text.is_empty()
        });
    Layout { categories, header }
}

/// As categorias e séries de um intervalo.
pub fn build_data(range: Range, cell: impl Fn(u32, u32) -> CellInfo) -> ChartData {
    let layout = layout(range, &cell);
    let first_row = range.row + u32::from(layout.header);
    let last_row = range.row + range.rows;
    let first_col = range.col + u32::from(layout.categories);
    let categories = (first_row..last_row)
        .map(|row| if layout.categories { cell(row, range.col).text } else { (row - first_row + 1).to_string() })
        .collect();
    let series = (first_col..range.col + range.cols)
        .enumerate()
        .map(|(index, col)| Series {
            name: if layout.header { cell(range.row, col).text } else { format!("Série {}", index + 1) },
            values: (first_row..last_row).map(|row| cell(row, col).number).collect(),
        })
        .collect();
    ChartData { categories, series, has_categories: layout.categories, has_header: layout.header }
}

// ---------------------------------------------------------------------------
// Gravação
// ---------------------------------------------------------------------------

/// Letras da coluna (`0` → `A`).
fn col_letters(col: u32) -> String {
    let mut n = col + 1;
    let mut letters = Vec::new();
    while n > 0 {
        n -= 1;
        letters.push(b'A' + (n % 26) as u8);
        n /= 26;
    }
    letters.reverse();
    String::from_utf8(letters).unwrap_or_default()
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// `'Planilha 1'!$A$1:$B$3`
fn reference(sheet_name: &str, col: u32, row: u32, cols: u32, rows: u32) -> String {
    let name = sheet_name.replace('\'', "''");
    let start = format!("${}${}", col_letters(col), row + 1);
    if cols == 1 && rows == 1 {
        return format!("'{name}'!{start}");
    }
    format!("'{name}'!{start}:${}${}", col_letters(col + cols - 1), row + rows)
}

fn str_cache(values: &[String]) -> String {
    let points: String =
        values.iter().enumerate().map(|(i, v)| format!("<c:pt idx=\"{i}\"><c:v>{}</c:v></c:pt>", escape(v))).collect();
    format!("<c:strCache><c:ptCount val=\"{}\"/>{points}</c:strCache>", values.len())
}

fn num_cache(values: &[Option<f64>]) -> String {
    let points: String = values
        .iter()
        .enumerate()
        .filter_map(|(i, v)| v.map(|v| format!("<c:pt idx=\"{i}\"><c:v>{v}</c:v></c:pt>")))
        .collect();
    format!(
        "<c:numCache><c:formatCode>General</c:formatCode><c:ptCount val=\"{}\"/>{points}</c:numCache>",
        values.len()
    )
}

fn fill(color: &str) -> String {
    format!("<a:solidFill><a:srgbClr val=\"{color}\"/></a:solidFill>")
}

/// O XML de um gráfico (`xl/charts/chartN.xml`).
fn chart_xml(chart: &Chart, data: &ChartData, sheet_name: &str) -> String {
    let layout_info = Layout { categories: data.has_categories, header: data.has_header };
    let first_row = chart.range.row + u32::from(layout_info.header);
    let first_col = chart.range.col + u32::from(layout_info.categories);
    let n = data.categories.len() as u32;

    let category_xml = if layout_info.categories {
        format!(
            "<c:cat><c:strRef><c:f>{}</c:f>{}</c:strRef></c:cat>",
            escape(&reference(sheet_name, chart.range.col, first_row, 1, n)),
            str_cache(&data.categories)
        )
    } else {
        String::new()
    };

    let series_xml: String = data
        .series
        .iter()
        .enumerate()
        .map(|(index, series)| {
            let col = first_col + index as u32;
            let color = PALETTE[index % PALETTE.len()];
            let name = if layout_info.header {
                format!(
                    "<c:tx><c:strRef><c:f>{}</c:f>{}</c:strRef></c:tx>",
                    escape(&reference(sheet_name, col, chart.range.row, 1, 1)),
                    str_cache(std::slice::from_ref(&series.name))
                )
            } else {
                format!("<c:tx><c:v>{}</c:v></c:tx>", escape(&series.name))
            };
            let look = match chart.kind {
                ChartKind::Line => format!(
                    "<c:spPr><a:ln w=\"28575\" cap=\"rnd\">{}<a:round/></a:ln></c:spPr><c:marker><c:symbol val=\"circle\"/><c:size val=\"5\"/><c:spPr>{}</c:spPr></c:marker>",
                    fill(color),
                    fill(color)
                ),
                ChartKind::Pie => {
                    let slices: String = (0..data.categories.len())
                        .map(|i| format!("<c:dPt><c:idx val=\"{i}\"/><c:bubble3D val=\"0\"/><c:spPr>{}</c:spPr></c:dPt>", fill(PALETTE[i % PALETTE.len()])))
                        .collect();
                    slices
                }
                _ => format!("<c:spPr>{}</c:spPr><c:invertIfNegative val=\"0\"/>", fill(color)),
            };
            // `look` tem o `spPr` primeiro, exceto na pizza, onde as fatias vêm depois do nome.
            let values = format!(
                "<c:val><c:numRef><c:f>{}</c:f>{}</c:numRef></c:val>",
                escape(&reference(sheet_name, col, first_row, 1, n)),
                num_cache(&series.values)
            );
            let smooth = if chart.kind == ChartKind::Line { "<c:smooth val=\"0\"/>" } else { "" };
            format!(
                "<c:ser><c:idx val=\"{index}\"/><c:order val=\"{index}\"/>{name}{look}{category_xml}{values}{smooth}</c:ser>"
            )
        })
        .collect();

    let text_style = "<c:txPr><a:bodyPr/><a:lstStyle/><a:p><a:pPr><a:defRPr sz=\"900\"/></a:pPr><a:endParaRPr lang=\"pt-BR\"/></a:p></c:txPr>";
    let (plot, axes) = match chart.kind {
        ChartKind::Pie => (
            format!("<c:pieChart><c:varyColors val=\"1\"/>{series_xml}<c:firstSliceAng val=\"0\"/></c:pieChart>"),
            String::new(),
        ),
        kind => {
            let (cat_pos, val_pos) = if kind == ChartKind::Bar { ("l", "b") } else { ("b", "l") };
            let body = match kind {
                ChartKind::Line => format!(
                    "<c:lineChart><c:grouping val=\"standard\"/><c:varyColors val=\"0\"/>{series_xml}<c:marker val=\"1\"/><c:axId val=\"111\"/><c:axId val=\"222\"/></c:lineChart>"
                ),
                _ => format!(
                    "<c:barChart><c:barDir val=\"{}\"/><c:grouping val=\"clustered\"/><c:varyColors val=\"0\"/>{series_xml}<c:gapWidth val=\"80\"/><c:axId val=\"111\"/><c:axId val=\"222\"/></c:barChart>",
                    if kind == ChartKind::Bar { "bar" } else { "col" }
                ),
            };
            let axes = format!(
                "<c:catAx><c:axId val=\"111\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"{cat_pos}\"/><c:numFmt formatCode=\"General\" sourceLinked=\"0\"/><c:majorTickMark val=\"none\"/><c:minorTickMark val=\"none\"/><c:tickLblPos val=\"nextTo\"/>{text_style}<c:crossAx val=\"222\"/><c:crosses val=\"autoZero\"/><c:auto val=\"1\"/><c:lblAlgn val=\"ctr\"/><c:lblOffset val=\"100\"/><c:noMultiLvlLbl val=\"0\"/></c:catAx>\
<c:valAx><c:axId val=\"222\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"{val_pos}\"/><c:majorGridlines><c:spPr><a:ln w=\"6350\"><a:solidFill><a:srgbClr val=\"E5E7EB\"/></a:solidFill></a:ln></c:spPr></c:majorGridlines><c:numFmt formatCode=\"General\" sourceLinked=\"0\"/><c:majorTickMark val=\"none\"/><c:minorTickMark val=\"none\"/><c:tickLblPos val=\"nextTo\"/>{text_style}<c:crossAx val=\"111\"/><c:crosses val=\"autoZero\"/><c:crossBetween val=\"between\"/></c:valAx>"
            );
            (body, axes)
        }
    };

    let title = if chart.title.is_empty() {
        "<c:autoTitleDeleted val=\"1\"/>".to_string()
    } else {
        format!(
            "<c:title><c:tx><c:rich><a:bodyPr/><a:lstStyle/><a:p><a:pPr><a:defRPr sz=\"1200\" b=\"1\"/></a:pPr><a:r><a:rPr lang=\"pt-BR\" sz=\"1200\" b=\"1\"/><a:t>{}</a:t></a:r></a:p></c:rich></c:tx><c:overlay val=\"0\"/></c:title><c:autoTitleDeleted val=\"0\"/>",
            escape(&chart.title)
        )
    };
    let legend = if data.series.len() > 1 || chart.kind == ChartKind::Pie {
        format!("<c:legend><c:legendPos val=\"b\"/><c:overlay val=\"0\"/>{text_style}</c:legend>")
    } else {
        String::new()
    };
    format!(
        "{XML_DECLARATION}\n<c:chartSpace xmlns:c=\"{NS_CHART}\" xmlns:a=\"{NS_DRAWING}\" xmlns:r=\"{NS_REL}\"><c:roundedCorners val=\"0\"/><c:chart>{title}<c:plotArea><c:layout/>{plot}{axes}</c:plotArea>{legend}<c:plotVisOnly val=\"1\"/><c:dispBlanksAs val=\"gap\"/></c:chart></c:chartSpace>"
    )
}

/// O XML do desenho de uma planilha, com um gráfico por âncora.
fn drawing_xml(charts: &[&Chart]) -> String {
    let anchors: String = charts
        .iter()
        .enumerate()
        .map(|(k, chart)| {
            format!(
                "<xdr:twoCellAnchor><xdr:from><xdr:col>{}</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>{}</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:to><xdr:col>{}</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>{}</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to><xdr:graphicFrame macro=\"\"><xdr:nvGraphicFramePr><xdr:cNvPr id=\"{}\" name=\"Gráfico {}\"/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr><xdr:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/></xdr:xfrm><a:graphic><a:graphicData uri=\"{NS_CHART}\"><c:chart xmlns:c=\"{NS_CHART}\" xmlns:r=\"{NS_REL}\" r:id=\"rId{}\"/></a:graphicData></a:graphic></xdr:graphicFrame><xdr:clientData/></xdr:twoCellAnchor>",
                chart.from_col,
                chart.from_row,
                chart.to_col + 1,
                chart.to_row + 1,
                k + 2,
                k + 1,
                k + 1
            )
        })
        .collect();
    format!(
        "{XML_DECLARATION}\n<xdr:wsDr xmlns:xdr=\"{NS_SHEET_DRAWING}\" xmlns:a=\"{NS_DRAWING}\">{anchors}</xdr:wsDr>"
    )
}

fn relationships(items: &[(String, &str, String)]) -> String {
    let body: String = items
        .iter()
        .map(|(id, kind, target)| format!("<Relationship Id=\"{id}\" Type=\"{kind}\" Target=\"{target}\"/>"))
        .collect();
    format!(
        "{XML_DECLARATION}\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{body}</Relationships>"
    )
}

/// Acrescenta os gráficos a um `.xlsx` gravado pelo motor. `sheet_names` está
/// na ordem das planilhas, e `charts` traz cada gráfico com os seus dados.
pub fn write_into(xlsx: Vec<u8>, sheet_names: &[String], charts: &[(Chart, ChartData)]) -> Result<Vec<u8>, String> {
    if charts.is_empty() {
        return Ok(xlsx);
    }
    let fail = |e: &dyn std::fmt::Display| format!("não foi possível gravar os gráficos: {e}");
    let mut archive = ZipArchive::new(Cursor::new(xlsx)).map_err(|e| fail(&e))?;
    let mut parts: Vec<(String, Vec<u8>)> = Vec::new();
    for index in 0..archive.len() {
        let mut file = archive.by_index(index).map_err(|e| fail(&e))?;
        if file.is_dir() {
            continue;
        }
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).map_err(|e| fail(&e))?;
        let name = file.name().map_err(|e| fail(&e))?.into_owned();
        parts.push((name, bytes));
    }
    let take = |parts: &mut Vec<(String, Vec<u8>)>, name: &str| -> Option<String> {
        parts.iter().position(|(n, _)| n == name).map(|i| String::from_utf8_lossy(&parts.remove(i).1).into_owned())
    };

    let mut content_types = take(&mut parts, "[Content_Types].xml").ok_or("o arquivo não tem [Content_Types].xml")?;
    let mut overrides = String::new();
    let mut chart_number = 0;
    let mut sheets: Vec<u32> = charts.iter().map(|(c, _)| c.sheet).collect();
    sheets.sort_unstable();
    sheets.dedup();
    for sheet in sheets {
        let sheet_name = sheet_names.get(sheet as usize).ok_or("planilha do gráfico não encontrada")?;
        let number = sheet + 1;
        let here: Vec<&(Chart, ChartData)> = charts.iter().filter(|(c, _)| c.sheet == sheet).collect();
        let mut drawing_rels = Vec::new();
        for (k, (chart, data)) in here.iter().enumerate() {
            chart_number += 1;
            parts.push((format!("xl/charts/chart{chart_number}.xml"), chart_xml(chart, data, sheet_name).into_bytes()));
            overrides.push_str(&format!("<Override PartName=\"/xl/charts/chart{chart_number}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.drawingml.chart+xml\"/>"));
            drawing_rels.push((format!("rId{}", k + 1), REL_CHART, format!("../charts/chart{chart_number}.xml")));
        }
        let only_charts: Vec<&Chart> = here.iter().map(|(c, _)| c).collect();
        parts.push((format!("xl/drawings/drawing{number}.xml"), drawing_xml(&only_charts).into_bytes()));
        parts.push((format!("xl/drawings/_rels/drawing{number}.xml.rels"), relationships(&drawing_rels).into_bytes()));
        overrides.push_str(&format!("<Override PartName=\"/xl/drawings/drawing{number}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.drawing+xml\"/>"));

        // A planilha aponta para o desenho.
        let sheet_part = format!("xl/worksheets/sheet{number}.xml");
        let mut sheet_xml = take(&mut parts, &sheet_part).ok_or("planilha do gráfico não encontrada no arquivo")?;
        let tag = format!("<drawing r:id=\"{DRAWING_REL_ID}\"/>");
        let at = sheet_xml.rfind("<extLst").or_else(|| sheet_xml.rfind("</worksheet>")).ok_or("planilha inválida")?;
        sheet_xml.insert_str(at, &tag);
        parts.push((sheet_part, sheet_xml.into_bytes()));

        let rels_part = format!("xl/worksheets/_rels/sheet{number}.xml.rels");
        let link = format!(
            "<Relationship Id=\"{DRAWING_REL_ID}\" Type=\"{REL_DRAWING}\" Target=\"../drawings/drawing{number}.xml\"/>"
        );
        let rels = match take(&mut parts, &rels_part) {
            Some(mut existing) => {
                let at = existing.rfind("</Relationships>").ok_or("relações inválidas")?;
                existing.insert_str(at, &link);
                existing
            }
            None => format!(
                "{XML_DECLARATION}\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{link}</Relationships>"
            ),
        };
        parts.push((rels_part, rels.into_bytes()));
    }
    let at = content_types.rfind("</Types>").ok_or("[Content_Types].xml inválido")?;
    content_types.insert_str(at, &overrides);
    parts.insert(0, ("[Content_Types].xml".to_string(), content_types.into_bytes()));

    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, bytes) in parts {
        writer.start_file(name, options).map_err(|e| fail(&e))?;
        writer.write_all(&bytes).map_err(|e| fail(&e))?;
    }
    Ok(writer.finish().map_err(|e| fail(&e))?.into_inner())
}

// ---------------------------------------------------------------------------
// Leitura
// ---------------------------------------------------------------------------

/// Um elemento XML, só com o que a leitura precisa.
#[derive(Default)]
struct Node {
    name: String,
    attrs: Vec<(String, String)>,
    children: Vec<Node>,
    text: String,
}

impl Node {
    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.iter().find(|(key, _)| key == name).map(|(_, value)| value.as_str())
    }

    fn child(&self, name: &str) -> Option<&Node> {
        self.children.iter().find(|child| child.name == name)
    }

    /// Todos os descendentes com o nome dado, em ordem.
    fn all<'a>(&'a self, name: &str, found: &mut Vec<&'a Node>) {
        for child in &self.children {
            if child.name == name {
                found.push(child);
            }
            child.all(name, found);
        }
    }

    fn first(&self, name: &str) -> Option<&Node> {
        let mut found = Vec::new();
        self.all(name, &mut found);
        found.into_iter().next()
    }
}

fn parse_xml(bytes: &[u8]) -> Option<Node> {
    let mut reader = Reader::from_reader(bytes);
    let mut stack = vec![Node::default()];
    let make = |element: &quick_xml::events::BytesStart| {
        let mut node = Node { name: element.local_name().as_ref().to_string(), ..Node::default() };
        for attr in element.attributes().flatten() {
            let key = attr.key.local_name().as_ref().to_string();
            let value =
                attr.normalized_value(quick_xml::XmlVersion::default()).map(|v| v.into_owned()).unwrap_or_default();
            node.attrs.push((key, value));
        }
        node
    };
    loop {
        match reader.read_event().ok()? {
            Event::Start(element) => stack.push(make(&element)),
            Event::Empty(element) => stack.last_mut()?.children.push(make(&element)),
            Event::End(_) => {
                let node = stack.pop()?;
                stack.last_mut()?.children.push(node);
            }
            Event::Text(text) => stack.last_mut()?.text.push_str(&text.xml10_content()),
            Event::GeneralRef(entity) => {
                let name = entity.as_ref();
                let decoded = match name {
                    "amp" => Some('&'),
                    "lt" => Some('<'),
                    "gt" => Some('>'),
                    "quot" => Some('"'),
                    "apos" => Some('\''),
                    _ => entity.resolve_char_ref().ok().flatten(),
                };
                if let Some(c) = decoded {
                    stack.last_mut()?.text.push(c);
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    let mut root = stack.pop()?;
    if !stack.is_empty() || root.children.len() != 1 {
        return None;
    }
    root.children.pop()
}

/// Resolve `target` em relação à pasta de `base` (`xl/worksheets/sheet1.xml`
/// + `../drawings/d.xml` → `xl/drawings/d.xml`).
fn resolve(base: &str, target: &str) -> String {
    let mut parts: Vec<&str> = if let Some(absolute) = target.strip_prefix('/') {
        return absolute.to_string();
    } else {
        base.split('/').collect()
    };
    parts.pop();
    for piece in target.split('/') {
        match piece {
            ".." => {
                parts.pop();
            }
            "." | "" => {}
            other => parts.push(other),
        }
    }
    parts.join("/")
}

/// Pasta de relações de um arquivo: `xl/a/b.xml` → `xl/a/_rels/b.xml.rels`.
fn rels_path(part: &str) -> String {
    match part.rsplit_once('/') {
        Some((dir, file)) => format!("{dir}/_rels/{file}.rels"),
        None => format!("_rels/{part}.rels"),
    }
}

/// Lê `A1` ou `$B$3` como (coluna, linha) a partir de zero.
fn parse_cell(text: &str) -> Option<(u32, u32)> {
    let text = text.replace('$', "");
    let split = text.find(|c: char| c.is_ascii_digit())?;
    let (letters, digits) = text.split_at(split);
    if letters.is_empty() || !letters.chars().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    let col = letters
        .to_ascii_uppercase()
        .bytes()
        .fold(0u32, |acc, b| acc.saturating_mul(26).saturating_add(u32::from(b - b'A') + 1))
        - 1;
    Some((col, digits.parse::<u32>().ok()?.checked_sub(1)?))
}

/// Lê `'Nome'!$A$1:$B$3` como a planilha e o intervalo (colunas e linhas, inclusive).
fn parse_reference(text: &str) -> Option<(String, (u32, u32, u32, u32))> {
    let (sheet, cells) = text.rsplit_once('!')?;
    let sheet = sheet
        .strip_prefix('\'')
        .and_then(|s| s.strip_suffix('\''))
        .map(|s| s.replace("''", "'"))
        .unwrap_or_else(|| sheet.to_string());
    let (first, last) = cells.split_once(':').unwrap_or((cells, cells));
    let (col1, row1) = parse_cell(first)?;
    let (col2, row2) = parse_cell(last)?;
    Some((sheet, (col1.min(col2), row1.min(row2), col1.max(col2), row1.max(row2))))
}

/// Gráficos que um arquivo traz. Os ids começam em 1; gráficos de outros
/// tipos, ou com dados que não estão numa planilha, ficam de fora.
pub fn read_from(xlsx: &[u8], sheet_names: &[String]) -> Vec<Chart> {
    read_inner(xlsx, sheet_names).unwrap_or_default()
}

fn read_inner(xlsx: &[u8], sheet_names: &[String]) -> Option<Vec<Chart>> {
    let mut archive = ZipArchive::new(Cursor::new(xlsx)).ok()?;
    let mut read = |name: &str| -> Option<Vec<u8>> {
        let mut file = archive.by_name(name).ok()?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).ok()?;
        Some(bytes)
    };
    let rels_of = |bytes: Option<Vec<u8>>| -> Vec<(String, String, String)> {
        let Some(root) = bytes.as_deref().and_then(parse_xml) else { return Vec::new() };
        root.children
            .iter()
            .map(|rel| {
                (
                    rel.attr("Id").unwrap_or_default().to_string(),
                    rel.attr("Type").unwrap_or_default().to_string(),
                    rel.attr("Target").unwrap_or_default().to_string(),
                )
            })
            .collect()
    };

    let workbook = parse_xml(&read("xl/workbook.xml")?)?;
    let workbook_rels = rels_of(read("xl/_rels/workbook.xml.rels"));
    let mut charts = Vec::new();
    for sheet_node in &workbook.first("sheets")?.children {
        let name = sheet_node.attr("name")?;
        let Some(sheet) = sheet_names.iter().position(|n| n == name) else { continue };
        let Some(target) =
            workbook_rels.iter().find(|(id, _, _)| Some(id.as_str()) == sheet_node.attr("id")).map(|r| r.2.clone())
        else {
            continue;
        };
        let part = resolve("xl/workbook.xml", &target);
        let sheet_rels = rels_of(read(&rels_path(&part)));
        let Some(drawing_target) = sheet_rels.iter().find(|r| r.1 == REL_DRAWING).map(|r| r.2.clone()) else {
            continue;
        };
        let drawing_part = resolve(&part, &drawing_target);
        let Some(drawing) = read(&drawing_part).as_deref().and_then(parse_xml) else { continue };
        let drawing_rels = rels_of(read(&rels_path(&drawing_part)));

        for anchor in &drawing.children {
            let Some(frame) = anchor.first("chart") else { continue };
            let Some(rel_id) = frame.attr("id") else { continue };
            let Some(chart_target) =
                drawing_rels.iter().find(|r| r.0 == rel_id && r.1 == REL_CHART).map(|r| r.2.clone())
            else {
                continue;
            };
            let Some(chart_xml) = read(&resolve(&drawing_part, &chart_target)).as_deref().and_then(parse_xml) else {
                continue;
            };
            let Some(chart) = read_chart(&chart_xml, anchor, name) else { continue };
            charts.push(Chart { id: charts.len() as u32 + 1, sheet: sheet as u32, ..chart });
        }
    }
    Some(charts)
}

/// Um gráfico a partir do XML do gráfico e da âncora no desenho.
fn read_chart(chart_xml: &Node, anchor: &Node, sheet_name: &str) -> Option<Chart> {
    let plot = chart_xml.first("plotArea")?;
    let kind = if plot.first("pieChart").is_some() {
        ChartKind::Pie
    } else if plot.first("lineChart").is_some() {
        ChartKind::Line
    } else {
        let bars = plot.first("barChart")?;
        if bars.child("barDir").and_then(|d| d.attr("val")) == Some("bar") { ChartKind::Bar } else { ChartKind::Column }
    };
    let title = chart_xml
        .child("chart")
        .and_then(|c| c.child("title"))
        .map(|title| {
            let mut texts = Vec::new();
            title.all("t", &mut texts);
            texts.iter().map(|t| t.text.as_str()).collect::<String>()
        })
        .unwrap_or_default();

    // Os dados: a menor área que cobre todas as referências à planilha do gráfico.
    let mut formulas = Vec::new();
    chart_xml.all("f", &mut formulas);
    let mut bounds: Option<(u32, u32, u32, u32)> = None;
    for formula in formulas {
        let Some((sheet, (c1, r1, c2, r2))) = parse_reference(&formula.text) else { continue };
        if sheet != sheet_name {
            continue;
        }
        bounds = Some(match bounds {
            None => (c1, r1, c2, r2),
            Some((a, b, c, d)) => (a.min(c1), b.min(r1), c.max(c2), d.max(r2)),
        });
    }
    let (c1, r1, c2, r2) = bounds?;
    let range = Range { col: c1, row: r1, cols: c2 - c1 + 1, rows: r2 - r1 + 1 };
    if range.cols > MAX_CHART_COLS || range.rows > MAX_CHART_ROWS {
        return None;
    }

    // A âncora: duas células, ou uma célula e um tamanho em EMU (9525 por pixel).
    let corner = |name: &str| -> Option<(u32, u32, u32, u32)> {
        let node = anchor.child(name)?;
        let number = |field: &str| node.child(field)?.text.trim().parse::<u32>().ok();
        Some((number("col")?, number("row")?, number("colOff")?, number("rowOff")?))
    };
    let (from_col, from_row, _, _) = corner("from")?;
    let (to_col, to_row) = match corner("to") {
        Some((col, row, col_off, row_off)) => (
            if col_off == 0 && col > from_col { col - 1 } else { col },
            if row_off == 0 && row > from_row { row - 1 } else { row },
        ),
        None => {
            let ext = anchor.child("ext")?;
            let size = |field: &str| ext.attr(field)?.parse::<u32>().ok();
            (from_col + (size("cx")? / 9525 / 90).max(1) - 1, from_row + (size("cy")? / 9525 / 20).max(1) - 1)
        }
    };
    Some(Chart { id: 0, sheet: 0, kind, title, range, from_col, from_row, to_col, to_row })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_references_and_paths() {
        assert_eq!(parse_reference("Planilha1!$B$2:$C$5"), Some(("Planilha1".into(), (1, 1, 2, 4))));
        assert_eq!(parse_reference("'Dados d''água'!A1"), Some(("Dados d'água".into(), (0, 0, 0, 0))));
        assert_eq!(parse_reference("sem planilha"), None);
        assert_eq!(parse_cell("AA10"), Some((26, 9)));
        assert_eq!(col_letters(26), "AA");
        assert_eq!(resolve("xl/worksheets/sheet1.xml", "../drawings/drawing1.xml"), "xl/drawings/drawing1.xml");
        assert_eq!(resolve("xl/workbook.xml", "/xl/worksheets/sheet1.xml"), "xl/worksheets/sheet1.xml");
        assert_eq!(rels_path("xl/worksheets/sheet1.xml"), "xl/worksheets/_rels/sheet1.xml.rels");
    }

    #[test]
    fn unreadable_files_have_no_charts() {
        assert!(read_from(b"isto nao e um zip", &["A".to_string()]).is_empty());
    }
}
