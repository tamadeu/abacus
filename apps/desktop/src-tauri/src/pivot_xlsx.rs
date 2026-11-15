//! Tabelas dinâmicas no formato nativo do Excel: cache de dados, registros e
//! definição da tabela, acrescentados ao `.xlsx` depois que o motor o salva.
//!
//! O cache é marcado com `refreshOnLoad`, então o Excel (e o LibreOffice)
//! recalculam a tabela ao abrir; as células já escritas pelo Abacus servem
//! de visualização até lá. Recursos que o Excel não tem do mesmo jeito
//! (agrupamentos, valores distintos, "primeiros N", ordem por valor) ficam
//! de fora: a tabela vai só como valores, mais a definição do Abacus.

use crate::charts::{col_letters, escape};
use crate::package;
use crate::pivot::{Agg, Group, Pivot, ShowAs, SortKind, Source, ValueField, value_title};
use crate::validation::parse_range_text;

const REL_BASE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const NS_MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const XML_DECLARATION: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#;
const CT_BASE: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml";

/// Tudo que a exportação precisa saber de uma tabela.
pub struct Export<'a> {
    pub pivot: &'a Pivot,
    pub source: &'a Source,
    pub source_sheet: String,
    /// Linhas de filtros no começo da grade.
    pub page_rows: usize,
    /// Linhas de cabeçalho da tabela (sem os filtros).
    pub header_rows: usize,
    pub label_cols: usize,
    /// Tamanho da grade inteira, filtros incluídos.
    pub height: usize,
    pub width: usize,
    /// Os nomes dos campos estão mesmo escritos na primeira linha da origem.
    pub headers_written: bool,
}

/// O Excel consegue representar esta definição?
pub fn supported(e: &Export) -> bool {
    let p = e.pivot;
    if !e.headers_written || e.width == 0 || e.height == 0 {
        return false;
    }
    if p.values.is_empty() && p.rows.is_empty() && p.cols.is_empty() {
        return false;
    }
    if p.rows.iter().chain(&p.cols).any(|f| f.group != Group::None || f.top.is_some()) {
        return false;
    }
    if p.rows.iter().chain(&p.cols).any(|f| matches!(f.sort, SortKind::ValueAsc | SortKind::ValueDesc)) {
        return false;
    }
    if p.values.iter().any(|v| v.agg == Agg::Distinct) {
        return false;
    }
    // O mesmo campo em duas áreas de eixo não existe no Excel.
    let mut axes: Vec<&str> = p.rows.iter().chain(&p.cols).map(|f| f.name.as_str()).collect();
    axes.extend(p.filters.iter().map(|f| f.field.as_str()));
    let total = axes.len();
    axes.sort_unstable();
    axes.dedup();
    if axes.len() != total {
        return false;
    }
    let known = |name: &str| e.source.headers.iter().any(|h| h == name);
    if !p.rows.iter().chain(&p.cols).all(|f| known(&f.name)) || !p.filters.iter().all(|f| known(&f.field)) {
        return false;
    }
    p.values.iter().all(|v| v.agg == Agg::Calc || known(&v.field))
}

/// Uma célula da origem, como o cache a guarda.
#[derive(Clone, PartialEq)]
enum Datum {
    Blank,
    Num(f64),
    Str(String),
}

fn datum(cell: &crate::pivot::SrcCell) -> Datum {
    if let Some(n) = cell.num {
        Datum::Num(n)
    } else if cell.text.is_empty() {
        Datum::Blank
    } else {
        Datum::Str(cell.text.clone())
    }
}

fn number(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 { format!("{}", n as i64) } else { format!("{n}") }
}

/// Um campo do cache: valores distintos (se for de eixo) e resumo do tipo.
struct CacheField {
    name: String,
    /// `Some` quando o campo é de eixo ou filtro: itens compartilhados, na ordem de aparição.
    items: Option<Vec<Datum>>,
    /// Nome mostrado de cada item (para casar com os filtros ocultos).
    item_text: Vec<String>,
    column: Vec<Datum>,
    /// Conta de um campo calculado; esses não têm dados na origem.
    formula: Option<String>,
}

fn shared_items_xml(field: &CacheField) -> String {
    let data: Vec<&Datum> = field.column.iter().collect();
    let has_str = data.iter().any(|d| matches!(d, Datum::Str(_)));
    let has_blank = data.iter().any(|d| matches!(d, Datum::Blank));
    let nums: Vec<f64> = data.iter().filter_map(|d| if let Datum::Num(n) = d { Some(*n) } else { None }).collect();
    let has_num = !nums.is_empty();
    let mut attrs = String::new();
    if !has_str && !has_blank && has_num {
        attrs.push_str(" containsSemiMixedTypes=\"0\"");
    }
    if !has_str {
        attrs.push_str(" containsString=\"0\"");
    }
    if has_blank {
        attrs.push_str(" containsBlank=\"1\"");
    }
    if has_num {
        attrs.push_str(" containsNumber=\"1\"");
        if nums.iter().all(|n| n.fract() == 0.0) {
            attrs.push_str(" containsInteger=\"1\"");
        }
        let min = nums.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = nums.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        attrs.push_str(&format!(" minValue=\"{}\" maxValue=\"{}\"", number(min), number(max)));
    }
    if has_str && has_num {
        attrs.push_str(" containsMixedTypes=\"1\"");
    }
    match &field.items {
        None => format!("<sharedItems{attrs}/>"),
        Some(items) => {
            let body: String = items.iter().map(item_xml).collect();
            format!("<sharedItems{attrs} count=\"{}\">{body}</sharedItems>", items.len())
        }
    }
}

fn item_xml(d: &Datum) -> String {
    match d {
        Datum::Blank => "<m/>".into(),
        Datum::Num(n) => format!("<n v=\"{}\"/>", number(*n)),
        Datum::Str(s) => format!("<s v=\"{}\"/>", escape(s)),
    }
}

/// `[Preço] * [Qtd]` vira `Preço * Qtd`, com aspas nos nomes que precisam delas.
fn excel_formula(formula: &str) -> String {
    let mut out = String::new();
    let mut rest = formula;
    while let Some(start) = rest.find('[') {
        out.push_str(&rest[..start]);
        let Some(end) = rest[start..].find(']') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let name = &rest[start + 1..start + end];
        if name.chars().all(|c| c.is_alphanumeric() || c == '_') && !name.starts_with(|c: char| c.is_ascii_digit()) {
            out.push_str(name);
        } else {
            out.push_str(&format!("'{name}'"));
        }
        rest = &rest[start + end + 1..];
    }
    out.push_str(rest);
    out
}

fn subtotal_name(agg: Agg) -> Option<&'static str> {
    match agg {
        Agg::Sum | Agg::Calc | Agg::Distinct => None,
        Agg::Count => Some("count"),
        Agg::CountNums => Some("countNums"),
        Agg::Average => Some("average"),
        Agg::Min => Some("min"),
        Agg::Max => Some("max"),
        Agg::Product => Some("product"),
        Agg::StdDev => Some("stdDev"),
        Agg::Var => Some("var"),
    }
}

fn number_format_id(vf: &ValueField) -> u32 {
    match vf.format.as_str() {
        "0" => 1,
        "0.00" => 2,
        "#,##0" => 3,
        "#,##0.00" => 4,
        "0%" => 9,
        "0.00%" => 10,
        "" if vf.show_as != ShowAs::Normal => 10,
        _ => 0,
    }
}

fn cache_fields(e: &Export) -> Vec<CacheField> {
    let p = e.pivot;
    let on_axis =
        |name: &str| p.rows.iter().chain(&p.cols).any(|f| f.name == name) || p.filters.iter().any(|f| f.field == name);
    let mut fields: Vec<CacheField> = e
        .source
        .headers
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let column: Vec<Datum> = e.source.rows.iter().map(|r| r.get(i).map_or(Datum::Blank, datum)).collect();
            let (items, item_text) = if on_axis(name) {
                let mut items: Vec<Datum> = Vec::new();
                let mut texts: Vec<String> = Vec::new();
                for (row, d) in e.source.rows.iter().zip(&column) {
                    if !items.contains(d) {
                        items.push(d.clone());
                        texts.push(row.get(i).map(|c| c.text.clone()).unwrap_or_default());
                    }
                }
                (Some(items), texts)
            } else {
                (None, Vec::new())
            };
            CacheField { name: name.clone(), items, item_text, column, formula: None }
        })
        .collect();
    for vf in p.values.iter().filter(|v| v.agg == Agg::Calc) {
        let mut name = value_title(vf);
        while fields.iter().any(|f| f.name == name) {
            name.push('_');
        }
        fields.push(CacheField {
            name,
            items: None,
            item_text: Vec::new(),
            column: Vec::new(),
            formula: Some(excel_formula(&vf.formula)),
        });
    }
    fields
}

fn cache_definition_xml(e: &Export, fields: &[CacheField], records_rel: &str) -> Result<String, String> {
    let range = parse_range_text(&e.pivot.source).ok_or("origem inválida")?;
    let source_ref = format!(
        "{}{}:{}{}",
        col_letters(range.col),
        range.row + 1,
        col_letters(range.col + range.cols - 1),
        range.row + range.rows
    );
    let mut body = String::new();
    for f in fields {
        match &f.formula {
            Some(formula) => body.push_str(&format!(
                "<cacheField name=\"{}\" numFmtId=\"0\" formula=\"{}\" databaseField=\"0\"/>",
                escape(&f.name),
                escape(formula)
            )),
            None => body.push_str(&format!(
                "<cacheField name=\"{}\" numFmtId=\"0\">{}</cacheField>",
                escape(&f.name),
                shared_items_xml(f)
            )),
        }
    }
    Ok(format!(
        "{XML_DECLARATION}\n<pivotCacheDefinition xmlns=\"{NS_MAIN}\" xmlns:r=\"{REL_BASE}\" r:id=\"{records_rel}\" refreshOnLoad=\"1\" refreshedBy=\"Abacus\" createdVersion=\"6\" refreshedVersion=\"6\" minRefreshableVersion=\"3\" recordCount=\"{}\"><cacheSource type=\"worksheet\"><worksheetSource ref=\"{source_ref}\" sheet=\"{}\"/></cacheSource><cacheFields count=\"{}\">{body}</cacheFields></pivotCacheDefinition>",
        e.source.rows.len(),
        escape(&e.source_sheet),
        fields.len()
    ))
}

fn cache_records_xml(e: &Export, fields: &[CacheField]) -> String {
    let mut body = String::new();
    for r in 0..e.source.rows.len() {
        body.push_str("<r>");
        for f in fields.iter().filter(|f| f.formula.is_none()) {
            let d = &f.column[r];
            match &f.items {
                Some(items) => {
                    let at = items.iter().position(|i| i == d).unwrap_or(0);
                    body.push_str(&format!("<x v=\"{at}\"/>"));
                }
                None => body.push_str(&item_xml(d)),
            }
        }
        body.push_str("</r>");
    }
    format!(
        "{XML_DECLARATION}\n<pivotCacheRecords xmlns=\"{NS_MAIN}\" xmlns:r=\"{REL_BASE}\" count=\"{}\">{body}</pivotCacheRecords>",
        e.source.rows.len()
    )
}

fn pivot_table_xml(e: &Export, fields: &[CacheField], cache_id: u32) -> String {
    let p = e.pivot;
    let index_of = |name: &str| fields.iter().position(|f| f.name == name).unwrap_or(0);
    let subtotals = p.subtotals;

    let mut pivot_fields = String::new();
    let calc_names: Vec<&str> = fields.iter().filter(|f| f.formula.is_some()).map(|f| f.name.as_str()).collect();
    for f in fields {
        let is_data =
            p.values.iter().any(|v| v.agg != Agg::Calc && v.field == f.name) || calc_names.contains(&f.name.as_str());
        let data_attr = if is_data { " dataField=\"1\"" } else { "" };
        if f.formula.is_some() {
            pivot_fields.push_str(
                "<pivotField dataField=\"1\" dragToRow=\"0\" dragToCol=\"0\" dragToPage=\"0\" showAll=\"0\" defaultSubtotal=\"0\"/>",
            );
            continue;
        }
        let axis = if p.rows.iter().any(|x| x.name == f.name) {
            Some("axisRow")
        } else if p.cols.iter().any(|x| x.name == f.name) {
            Some("axisCol")
        } else if p.filters.iter().any(|x| x.field == f.name) {
            Some("axisPage")
        } else {
            None
        };
        let Some(axis) = axis else {
            pivot_fields.push_str(&format!("<pivotField{data_attr} showAll=\"0\"/>"));
            continue;
        };
        let sort = p
            .rows
            .iter()
            .chain(&p.cols)
            .find(|x| x.name == f.name)
            .map(|x| if x.sort == SortKind::LabelDesc { " sortType=\"descending\"" } else { " sortType=\"ascending\"" })
            .unwrap_or("");
        let hidden: Vec<&String> = p.filters.iter().filter(|x| x.field == f.name).flat_map(|x| &x.hidden).collect();
        let mut items = String::new();
        for k in 0..f.items.as_ref().map_or(0, Vec::len) {
            if hidden.iter().any(|h| **h == f.item_text[k]) {
                items.push_str(&format!("<item h=\"1\" x=\"{k}\"/>"));
            } else {
                items.push_str(&format!("<item x=\"{k}\"/>"));
            }
        }
        let default_subtotal = if subtotals || axis == "axisPage" {
            items.push_str("<item t=\"default\"/>");
            ""
        } else {
            " defaultSubtotal=\"0\""
        };
        let count = f.items.as_ref().map_or(0, Vec::len) + usize::from(default_subtotal.is_empty());
        pivot_fields.push_str(&format!(
            "<pivotField axis=\"{axis}\"{data_attr} compact=\"0\" outline=\"0\" showAll=\"0\"{sort}{default_subtotal}><items count=\"{count}\">{items}</items></pivotField>"
        ));
    }

    let row_fields = if p.rows.is_empty() {
        String::new()
    } else {
        let body: String = p.rows.iter().map(|f| format!("<field x=\"{}\"/>", index_of(&f.name))).collect();
        format!("<rowFields count=\"{}\">{body}</rowFields>", p.rows.len())
    };
    let multi_values = p.values.len() > 1;
    let col_fields = {
        let mut body: String = p.cols.iter().map(|f| format!("<field x=\"{}\"/>", index_of(&f.name))).collect();
        let mut count = p.cols.len();
        if multi_values {
            body.push_str("<field x=\"-2\"/>");
            count += 1;
        }
        if count == 0 { String::new() } else { format!("<colFields count=\"{count}\">{body}</colFields>") }
    };
    let page_fields = if p.filters.is_empty() {
        String::new()
    } else {
        let body: String =
            p.filters.iter().map(|f| format!("<pageField fld=\"{}\" hier=\"-1\"/>", index_of(&f.field))).collect();
        format!("<pageFields count=\"{}\">{body}</pageFields>", p.filters.len())
    };

    let mut used_names: Vec<String> = fields.iter().map(|f| f.name.clone()).collect();
    let mut data_fields = String::new();
    let mut calc_seen = 0;
    for vf in &p.values {
        let fld = if vf.agg == Agg::Calc {
            let at = fields.len() - calc_names.len() + calc_seen;
            calc_seen += 1;
            at
        } else {
            index_of(&vf.field)
        };
        // O nome de um campo de valor não pode repetir o de um campo da origem.
        let mut name = value_title(vf);
        while used_names.contains(&name) {
            name.push(' ');
        }
        used_names.push(name.clone());
        let subtotal = subtotal_name(vf.agg).map(|s| format!(" subtotal=\"{s}\"")).unwrap_or_default();
        let show = match vf.show_as {
            ShowAs::Normal => "",
            ShowAs::PctTotal => " showDataAs=\"percentOfTotal\"",
            ShowAs::PctRow => " showDataAs=\"percentOfRow\"",
            ShowAs::PctCol => " showDataAs=\"percentOfCol\"",
        };
        data_fields.push_str(&format!(
            "<dataField name=\"{}\" fld=\"{fld}\"{subtotal}{show} baseField=\"0\" baseItem=\"0\" numFmtId=\"{}\"/>",
            escape(&name),
            number_format_id(vf)
        ));
    }
    let data_fields = if p.values.is_empty() {
        String::new()
    } else {
        format!("<dataFields count=\"{}\">{data_fields}</dataFields>", p.values.len())
    };

    let top = p.row + e.page_rows as u32;
    let last_row = top + e.height as u32;
    let table_ref =
        format!("{}{}:{}{}", col_letters(p.col), top + 1, col_letters(p.col + e.width as u32 - 1), last_row);
    let first_header = u32::from(!p.cols.is_empty());
    let page_attrs = if p.filters.is_empty() {
        String::new()
    } else {
        format!(" rowPageCount=\"{}\" colPageCount=\"1\"", p.filters.len())
    };
    let totals = format!(
        "{}{}",
        if p.grand_rows { "" } else { " colGrandTotals=\"0\"" },
        if p.grand_cols { "" } else { " rowGrandTotals=\"0\"" }
    );
    format!(
        "{XML_DECLARATION}\n<pivotTableDefinition xmlns=\"{NS_MAIN}\" name=\"TabelaDinamica{}\" cacheId=\"{cache_id}\" applyNumberFormats=\"0\" applyBorderFormats=\"0\" applyFontFormats=\"0\" applyPatternFormats=\"0\" applyAlignmentFormats=\"0\" applyWidthHeightFormats=\"1\" dataCaption=\"Valores\" updatedVersion=\"6\" minRefreshableVersion=\"3\" useAutoFormatting=\"1\" itemPrintTitles=\"1\" createdVersion=\"6\" indent=\"0\" compact=\"0\" compactData=\"0\" outline=\"0\" outlineData=\"0\" multipleFieldFilters=\"0\"{totals}><location ref=\"{table_ref}\" firstHeaderRow=\"{first_header}\" firstDataRow=\"{}\" firstDataCol=\"{}\"{page_attrs}/><pivotFields count=\"{}\">{pivot_fields}</pivotFields>{row_fields}{col_fields}{page_fields}{data_fields}<pivotTableStyleInfo name=\"PivotStyleLight16\" showRowHeaders=\"1\" showColHeaders=\"1\" showRowStripes=\"0\" showColStripes=\"0\" showLastColumn=\"1\"/></pivotTableDefinition>",
        p.id,
        e.header_rows.max(1),
        e.label_cols,
        fields.len()
    )
}

fn relationships(items: &[(String, String, String)]) -> String {
    let body: String = items
        .iter()
        .map(|(id, kind, target)| format!("<Relationship Id=\"{id}\" Type=\"{REL_BASE}/{kind}\" Target=\"{target}\"/>"))
        .collect();
    format!(
        "{XML_DECLARATION}\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{body}</Relationships>"
    )
}

/// Acrescenta uma relação num arquivo `.rels`, criando-o se não existir.
fn add_relationship(
    parts: &mut Vec<(String, Vec<u8>)>,
    rels_part: &str,
    id: &str,
    kind: &str,
    target: &str,
) -> Result<(), String> {
    let link = format!("<Relationship Id=\"{id}\" Type=\"{REL_BASE}/{kind}\" Target=\"{target}\"/>");
    let text = match package::take_text(parts, rels_part) {
        Some(mut existing) => {
            let at = existing.rfind("</Relationships>").ok_or("relações inválidas")?;
            existing.insert_str(at, &link);
            existing
        }
        None => format!(
            "{XML_DECLARATION}\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{link}</Relationships>"
        ),
    };
    parts.push((rels_part.to_string(), text.into_bytes()));
    Ok(())
}

/// Grava as tabelas que o Excel entende. As demais continuam só como valores.
pub fn write_into(xlsx: Vec<u8>, sheet_names: &[String], exports: &[Export]) -> Result<Vec<u8>, String> {
    let exports: Vec<&Export> = exports.iter().filter(|e| supported(e)).collect();
    if exports.is_empty() {
        return Ok(xlsx);
    }
    let sheet_files = package::sheet_parts(&xlsx, sheet_names);
    let mut parts = package::read_parts(&xlsx)?;
    let mut content_types =
        package::take_text(&mut parts, "[Content_Types].xml").ok_or("o arquivo não tem [Content_Types].xml")?;
    let mut overrides = String::new();
    let mut caches = String::new();
    let mut workbook_links = Vec::new();

    for (k, e) in exports.iter().enumerate() {
        let n = k as u32 + 1;
        let Some((_, sheet_part)) = sheet_files.iter().find(|(i, _)| *i == e.pivot.sheet as usize) else {
            continue;
        };
        let fields = cache_fields(e);
        parts.push((
            format!("xl/pivotCache/pivotCacheDefinition{n}.xml"),
            cache_definition_xml(e, &fields, "rId1")?.into_bytes(),
        ));
        parts.push((format!("xl/pivotCache/pivotCacheRecords{n}.xml"), cache_records_xml(e, &fields).into_bytes()));
        parts.push((
            format!("xl/pivotCache/_rels/pivotCacheDefinition{n}.xml.rels"),
            relationships(&[("rId1".into(), "pivotCacheRecords".into(), format!("pivotCacheRecords{n}.xml"))])
                .into_bytes(),
        ));
        parts.push((format!("xl/pivotTables/pivotTable{n}.xml"), pivot_table_xml(e, &fields, n).into_bytes()));
        parts.push((
            format!("xl/pivotTables/_rels/pivotTable{n}.xml.rels"),
            relationships(&[(
                "rId1".into(),
                "pivotCacheDefinition".into(),
                format!("../pivotCache/pivotCacheDefinition{n}.xml"),
            )])
            .into_bytes(),
        ));
        for (name, kind) in [
            (format!("/xl/pivotCache/pivotCacheDefinition{n}.xml"), "pivotCacheDefinition"),
            (format!("/xl/pivotCache/pivotCacheRecords{n}.xml"), "pivotCacheRecords"),
            (format!("/xl/pivotTables/pivotTable{n}.xml"), "pivotTable"),
        ] {
            overrides.push_str(&format!("<Override PartName=\"{name}\" ContentType=\"{CT_BASE}.{kind}+xml\"/>"));
        }
        let cache_rel = format!("rIdAbacusPivotCache{n}");
        caches.push_str(&format!("<pivotCache cacheId=\"{n}\" r:id=\"{cache_rel}\"/>"));
        workbook_links.push((cache_rel, format!("pivotCache/pivotCacheDefinition{n}.xml")));

        let rels_part = package_rels_path(sheet_part);
        add_relationship(
            &mut parts,
            &rels_part,
            &format!("rIdAbacusPivotTable{n}"),
            "pivotTable",
            &format!("../pivotTables/pivotTable{n}.xml"),
        )?;
    }

    // Workbook: lista de caches e as relações para eles.
    let mut workbook = package::take_text(&mut parts, "xl/workbook.xml").ok_or("o arquivo não tem workbook.xml")?;
    let at = workbook.rfind("<extLst").or_else(|| workbook.rfind("</workbook>")).ok_or("workbook inválido")?;
    workbook.insert_str(at, &format!("<pivotCaches>{caches}</pivotCaches>"));
    if !workbook.contains("xmlns:r=") {
        workbook = workbook.replacen("<workbook ", &format!("<workbook xmlns:r=\"{REL_BASE}\" "), 1);
    }
    parts.push(("xl/workbook.xml".to_string(), workbook.into_bytes()));
    for (id, target) in workbook_links {
        add_relationship(&mut parts, "xl/_rels/workbook.xml.rels", &id, "pivotCacheDefinition", &target)?;
    }

    let at = content_types.rfind("</Types>").ok_or("[Content_Types].xml inválido")?;
    content_types.insert_str(at, &overrides);
    parts.insert(0, ("[Content_Types].xml".to_string(), content_types.into_bytes()));
    package::write_parts(parts)
}

fn package_rels_path(part: &str) -> String {
    match part.rsplit_once('/') {
        Some((dir, file)) => format!("{dir}/_rels/{file}.rels"),
        None => format!("_rels/{part}.rels"),
    }
}
