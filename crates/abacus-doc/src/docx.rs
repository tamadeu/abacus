use std::io::{Cursor, Read, Write};

use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

use crate::model::{Align, Block, Document, ParaKind, Paragraph, Run};
use crate::xml::{self, Node, escape};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

// ---------------------------------------------------------------------------
// Leitura
// ---------------------------------------------------------------------------

fn part(archive: &mut ZipArchive<Cursor<&[u8]>>, name: &str) -> Option<Vec<u8>> {
    let mut file = archive.by_name(name).ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    Some(bytes)
}

/// Lê um `.docx`.
pub fn read_docx(bytes: &[u8]) -> Result<Document, String> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(|e| format!("arquivo inválido: {e}"))?;
    let body = part(&mut archive, "word/document.xml").ok_or("o arquivo não tem texto (word/document.xml)")?;
    let root = xml::parse(&body).ok_or("o texto do documento está corrompido")?;
    let numbering = part(&mut archive, "word/numbering.xml").and_then(|b| xml::parse(&b));
    let bullets = bullet_numbers(numbering.as_ref());

    let mut doc = Document::default();
    if let Some(body) = root.first("body") {
        collect(body, &bullets, &mut doc);
    }
    if doc.blocks.is_empty() {
        doc.blocks.push(Block::Paragraph(Paragraph::default()));
    }
    Ok(doc)
}

/// Os `numId` cuja lista, no primeiro nível, tem marcadores (e não números).
fn bullet_numbers(numbering: Option<&Node>) -> Vec<(String, bool)> {
    let Some(numbering) = numbering else { return Vec::new() };
    let is_bullet = |abstract_id: &str| {
        numbering
            .children
            .iter()
            .filter(|n| n.name == "abstractNum" && n.attr("abstractNumId") == Some(abstract_id))
            .flat_map(|n| n.children.iter())
            .find(|lvl| lvl.name == "lvl" && lvl.attr("ilvl").is_none_or(|i| i == "0"))
            .and_then(|lvl| lvl.first("numFmt"))
            .and_then(|fmt| fmt.attr("val"))
            .is_some_and(|fmt| fmt == "bullet")
    };
    numbering
        .children
        .iter()
        .filter(|n| n.name == "num")
        .filter_map(|num| {
            let id = num.attr("numId")?.to_string();
            let abstract_id = num.first("abstractNumId")?.attr("val")?;
            Some((id, is_bullet(abstract_id)))
        })
        .collect()
}

/// Percorre o corpo; parágrafos dentro de tabelas ou caixas entram como parágrafos comuns.
fn collect(node: &Node, bullets: &[(String, bool)], doc: &mut Document) {
    for child in &node.children {
        match child.name.as_str() {
            "p" => doc.blocks.push(Block::Paragraph(paragraph(child, bullets))),
            "tbl" | "tr" | "tc" | "sdt" | "sdtContent" => collect(child, bullets, doc),
            _ => {}
        }
    }
}

fn on(node: Option<&Node>) -> bool {
    node.is_some_and(|n| !matches!(n.attr("val"), Some("0" | "false" | "off")))
}

fn paragraph(node: &Node, bullets: &[(String, bool)]) -> Paragraph {
    let mut out = Paragraph::default();
    if let Some(props) = node.first("pPr") {
        if let Some(style) = props.first("pStyle").and_then(|s| s.attr("val")) {
            out.kind = style_kind(style);
        }
        if let Some(num) = props.first("numPr")
            && let Some(id) = num.first("numId").and_then(|n| n.attr("val"))
            && id != "0"
        {
            let bullet = bullets.iter().find(|(n, _)| n == id).is_none_or(|(_, b)| *b);
            out.kind = if bullet { ParaKind::Bullet } else { ParaKind::Numbered };
        }
        out.align = match props.first("jc").and_then(|j| j.attr("val")) {
            Some("center") => Align::Center,
            Some("right" | "end") => Align::Right,
            Some("both" | "distribute") => Align::Justify,
            _ => Align::Left,
        };
    }
    runs(node, &mut out.runs);
    out
}

fn style_kind(style: &str) -> ParaKind {
    let lower = style.to_lowercase();
    if lower == "title" {
        return ParaKind::Heading(1);
    }
    if let Some(level) = lower.strip_prefix("heading").or_else(|| lower.strip_prefix("título"))
        && let Ok(n) = level.trim().parse::<u8>()
    {
        return ParaKind::Heading(n.clamp(1, 6));
    }
    ParaKind::Normal
}

fn runs(node: &Node, out: &mut Vec<Run>) {
    for child in &node.children {
        match child.name.as_str() {
            "r" => {
                let mut run = Run::default();
                if let Some(props) = child.first("rPr") {
                    run.bold = on(props.first("b"));
                    run.italic = on(props.first("i"));
                    run.strike = on(props.first("strike"));
                    run.underline = props.first("u").is_some_and(|u| !matches!(u.attr("val"), Some("none")));
                }
                for item in &child.children {
                    match item.name.as_str() {
                        "t" => run.text.push_str(&item.text),
                        "tab" => run.text.push('\t'),
                        "br" | "cr" => run.text.push('\n'),
                        _ => {}
                    }
                }
                if !run.text.is_empty() {
                    push_run(out, run);
                }
            }
            // Links, campos e marcas de revisão: só o texto de dentro.
            "hyperlink" | "smartTag" | "ins" | "fldSimple" | "sdt" | "sdtContent" => runs(child, out),
            _ => {}
        }
    }
}

/// Junta trechos vizinhos com a mesma formatação.
fn push_run(out: &mut Vec<Run>, run: Run) {
    if let Some(last) = out.last_mut()
        && (last.bold, last.italic, last.underline, last.strike) == (run.bold, run.italic, run.underline, run.strike)
    {
        last.text.push_str(&run.text);
    } else {
        out.push(run);
    }
}

// ---------------------------------------------------------------------------
// Gravação
// ---------------------------------------------------------------------------

/// Grava o documento como `.docx`.
pub fn write_docx(doc: &Document) -> Result<Vec<u8>, String> {
    let (body, lists) = body_xml(doc);
    let parts: Vec<(&str, String)> = vec![
        ("[Content_Types].xml", CONTENT_TYPES.to_string()),
        ("_rels/.rels", ROOT_RELS.to_string()),
        (
            "word/document.xml",
            format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:document xmlns:w=\"{W}\"><w:body>{body}<w:sectPr><w:pgSz w:w=\"11906\" w:h=\"16838\"/><w:pgMar w:top=\"1440\" w:right=\"1440\" w:bottom=\"1440\" w:left=\"1440\" w:header=\"708\" w:footer=\"708\" w:gutter=\"0\"/></w:sectPr></w:body></w:document>"
            ),
        ),
        ("word/_rels/document.xml.rels", DOCUMENT_RELS.to_string()),
        ("word/styles.xml", styles_xml()),
        ("word/numbering.xml", numbering_xml(lists)),
    ];
    let fail = |e: &dyn std::fmt::Display| format!("não foi possível gravar o arquivo: {e}");
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, content) in parts {
        writer.start_file(name, options).map_err(|e| fail(&e))?;
        writer.write_all(content.as_bytes()).map_err(|e| fail(&e))?;
    }
    Ok(writer.finish().map_err(|e| fail(&e))?.into_inner())
}

/// Os parágrafos em XML e quantas listas numeradas há (cada uma recomeça em 1).
fn body_xml(doc: &Document) -> (String, u32) {
    let mut out = String::new();
    let mut lists = 0;
    let mut in_numbered = false;
    for Block::Paragraph(p) in &doc.blocks {
        let numbered = p.kind == ParaKind::Numbered;
        if numbered && !in_numbered {
            lists += 1;
        }
        in_numbered = numbered;
        out.push_str("<w:p>");
        let mut props = String::new();
        match p.kind {
            ParaKind::Normal => {}
            ParaKind::Heading(n) => props.push_str(&format!("<w:pStyle w:val=\"Heading{}\"/>", n.clamp(1, 6))),
            ParaKind::Bullet => props.push_str(
                "<w:pStyle w:val=\"ListParagraph\"/><w:numPr><w:ilvl w:val=\"0\"/><w:numId w:val=\"1\"/></w:numPr>",
            ),
            ParaKind::Numbered => props.push_str(&format!(
                "<w:pStyle w:val=\"ListParagraph\"/><w:numPr><w:ilvl w:val=\"0\"/><w:numId w:val=\"{}\"/></w:numPr>",
                lists + 1
            )),
        }
        match p.align {
            Align::Left => {}
            Align::Center => props.push_str("<w:jc w:val=\"center\"/>"),
            Align::Right => props.push_str("<w:jc w:val=\"right\"/>"),
            Align::Justify => props.push_str("<w:jc w:val=\"both\"/>"),
        }
        if !props.is_empty() {
            out.push_str(&format!("<w:pPr>{props}</w:pPr>"));
        }
        for run in &p.runs {
            out.push_str(&run_xml(run));
        }
        out.push_str("</w:p>");
    }
    (out, lists)
}

fn run_xml(run: &Run) -> String {
    let mut props = String::new();
    if run.bold {
        props.push_str("<w:b/>");
    }
    if run.italic {
        props.push_str("<w:i/>");
    }
    if run.strike {
        props.push_str("<w:strike/>");
    }
    if run.underline {
        props.push_str("<w:u w:val=\"single\"/>");
    }
    let mut out = String::from("<w:r>");
    if !props.is_empty() {
        out.push_str(&format!("<w:rPr>{props}</w:rPr>"));
    }
    // Tabulações e quebras viram marcas próprias; o resto, texto.
    let mut text = String::new();
    let flush = |text: &mut String, out: &mut String| {
        if !text.is_empty() {
            out.push_str(&format!("<w:t xml:space=\"preserve\">{}</w:t>", escape(text)));
            text.clear();
        }
    };
    for c in run.text.chars() {
        match c {
            '\t' => {
                flush(&mut text, &mut out);
                out.push_str("<w:tab/>");
            }
            '\n' => {
                flush(&mut text, &mut out);
                out.push_str("<w:br/>");
            }
            c => text.push(c),
        }
    }
    flush(&mut text, &mut out);
    out.push_str("</w:r>");
    out
}

fn styles_xml() -> String {
    let mut out = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:styles xmlns:w=\"{W}\">\
         <w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii=\"Calibri\" w:hAnsi=\"Calibri\" w:cs=\"Calibri\"/><w:sz w:val=\"22\"/><w:szCs w:val=\"22\"/><w:lang w:val=\"pt-BR\"/></w:rPr></w:rPrDefault>\
         <w:pPrDefault><w:pPr><w:spacing w:after=\"160\" w:line=\"259\" w:lineRule=\"auto\"/></w:pPr></w:pPrDefault></w:docDefaults>\
         <w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/><w:qFormat/></w:style>\
         <w:style w:type=\"paragraph\" w:styleId=\"ListParagraph\"><w:name w:val=\"List Paragraph\"/><w:basedOn w:val=\"Normal\"/><w:qFormat/><w:pPr><w:spacing w:after=\"60\"/><w:ind w:left=\"720\"/></w:pPr></w:style>"
    );
    let sizes = [40, 32, 28, 26, 24, 22];
    for (i, size) in sizes.iter().enumerate() {
        let n = i + 1;
        out.push_str(&format!(
            "<w:style w:type=\"paragraph\" w:styleId=\"Heading{n}\"><w:name w:val=\"heading {n}\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/><w:qFormat/>\
             <w:pPr><w:keepNext/><w:spacing w:before=\"240\" w:after=\"80\"/><w:outlineLvl w:val=\"{}\"/></w:pPr><w:rPr><w:b/><w:sz w:val=\"{size}\"/><w:szCs w:val=\"{size}\"/></w:rPr></w:style>",
            n - 1
        ));
    }
    out.push_str("</w:styles>");
    out
}

/// `numId` 1 são marcadores; de 2 em diante, uma lista numerada cada.
fn numbering_xml(lists: u32) -> String {
    let mut out = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:numbering xmlns:w=\"{W}\">\
         <w:abstractNum w:abstractNumId=\"1\"><w:multiLevelType w:val=\"hybridMultilevel\"/>\
         <w:lvl w:ilvl=\"0\"><w:start w:val=\"1\"/><w:numFmt w:val=\"bullet\"/><w:lvlText w:val=\"•\"/><w:lvlJc w:val=\"left\"/><w:pPr><w:ind w:left=\"720\" w:hanging=\"360\"/></w:pPr></w:lvl></w:abstractNum>\
         <w:abstractNum w:abstractNumId=\"2\"><w:multiLevelType w:val=\"hybridMultilevel\"/>\
         <w:lvl w:ilvl=\"0\"><w:start w:val=\"1\"/><w:numFmt w:val=\"decimal\"/><w:lvlText w:val=\"%1.\"/><w:lvlJc w:val=\"left\"/><w:pPr><w:ind w:left=\"720\" w:hanging=\"360\"/></w:pPr></w:lvl></w:abstractNum>\
         <w:num w:numId=\"1\"><w:abstractNumId w:val=\"1\"/></w:num>"
    );
    for n in 0..lists {
        out.push_str(&format!(
            "<w:num w:numId=\"{}\"><w:abstractNumId w:val=\"2\"/><w:lvlOverride w:ilvl=\"0\"><w:startOverride w:val=\"1\"/></w:lvlOverride></w:num>",
            n + 2
        ));
    }
    out.push_str("</w:numbering>");
    out
}

const CONTENT_TYPES: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/><Override PartName=\"/word/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml\"/><Override PartName=\"/word/numbering.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml\"/></Types>";

const ROOT_RELS: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/></Relationships>";

const DOCUMENT_RELS: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/><Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering\" Target=\"numbering.xml\"/></Relationships>";

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Document {
        let bold = |t: &str| Run { text: t.into(), bold: true, ..Run::default() };
        let plain = |t: &str| Run { text: t.into(), ..Run::default() };
        let mixed = Paragraph {
            runs: vec![
                plain("Texto com "),
                bold("negrito"),
                Run { text: " e itálico & <símbolos>".into(), italic: true, underline: true, ..Run::default() },
            ],
            ..Paragraph::default()
        };
        let item = |kind, t: &str| Paragraph { kind, ..Paragraph::text(t) };
        Document {
            blocks: vec![
                Block::Paragraph(Paragraph {
                    kind: ParaKind::Heading(1),
                    align: Align::Center,
                    ..Paragraph::text("Relatório")
                }),
                Block::Paragraph(mixed),
                Block::Paragraph(item(ParaKind::Bullet, "um")),
                Block::Paragraph(item(ParaKind::Bullet, "dois")),
                Block::Paragraph(item(ParaKind::Numbered, "primeiro")),
                Block::Paragraph(item(ParaKind::Numbered, "segundo")),
                Block::Paragraph(Paragraph::text("meio")),
                Block::Paragraph(item(ParaKind::Numbered, "recomeça")),
                Block::Paragraph(Paragraph {
                    align: Align::Justify,
                    runs: vec![Run { text: "linha 1\nlinha 2\tcom tab".into(), strike: true, ..Run::default() }],
                    ..Paragraph::default()
                }),
                Block::Paragraph(Paragraph::default()),
            ],
        }
    }

    #[test]
    fn docx_round_trip_keeps_the_model() {
        let doc = sample();
        let bytes = write_docx(&doc).unwrap();
        let back = read_docx(&bytes).unwrap();
        assert_eq!(back, doc);
    }

    #[test]
    fn numbered_lists_restart() {
        let bytes = write_docx(&sample()).unwrap();
        let mut archive = ZipArchive::new(Cursor::new(bytes.as_slice())).unwrap();
        let numbering = String::from_utf8(part(&mut archive, "word/numbering.xml").unwrap()).unwrap();
        // Uma lista de marcadores e duas numeradas.
        assert_eq!(numbering.matches("<w:num ").count(), 3);
    }

    #[test]
    fn reads_what_word_writes() {
        // Marcas que o Word usa: estilos "Heading 1"/"Título", negrito desligado, links e quebras.
        let xml = format!(
            "<w:document xmlns:w=\"{W}\"><w:body>\
             <w:p><w:pPr><w:pStyle w:val=\"Heading2\"/></w:pPr><w:r><w:t>Seção</w:t></w:r></w:p>\
             <w:p><w:r><w:rPr><w:b w:val=\"0\"/></w:rPr><w:t>sem negrito</w:t></w:r><w:hyperlink><w:r><w:rPr><w:b/></w:rPr><w:t> link</w:t></w:r></w:hyperlink></w:p>\
             <w:tbl><w:tr><w:tc><w:p><w:r><w:t>célula</w:t></w:r></w:p></w:tc></w:tr></w:tbl>\
             </w:body></w:document>"
        );
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        writer.start_file("word/document.xml", SimpleFileOptions::default()).unwrap();
        writer.write_all(xml.as_bytes()).unwrap();
        let bytes = writer.finish().unwrap().into_inner();
        let doc = read_docx(&bytes).unwrap();
        let Block::Paragraph(first) = &doc.blocks[0];
        assert_eq!(first.kind, ParaKind::Heading(2));
        let Block::Paragraph(second) = &doc.blocks[1];
        assert_eq!(second.runs.len(), 2);
        assert!(!second.runs[0].bold && second.runs[1].bold);
        assert_eq!(doc.plain(), "Seção\nsem negrito link\ncélula");
    }

    #[test]
    fn model_json_has_the_shape_the_editor_sends() {
        let json = r#"{"blocks":[
            {"Paragraph":{"kind":{"Heading":1},"align":"Left","runs":[{"text":"Oi","bold":true,"italic":false,"underline":false,"strike":false}]}},
            {"Paragraph":{"kind":"Bullet","align":"Center","runs":[]}}]}"#;
        let doc: Document = serde_json::from_str(json).unwrap();
        let Block::Paragraph(first) = &doc.blocks[0];
        assert_eq!(first.kind, ParaKind::Heading(1));
        assert!(first.runs[0].bold);
        assert_eq!(read_docx(&write_docx(&doc).unwrap()).unwrap(), doc);
    }

    #[test]
    fn rejects_files_that_are_not_docx() {
        assert!(read_docx(b"nao e zip").is_err());
    }
}
