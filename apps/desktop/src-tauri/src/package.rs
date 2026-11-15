//! Leitura e reescrita do pacote `.xlsx` (um zip de partes XML), para
//! acrescentar o que o IronCalc não grava: gráficos e validação de dados.

use std::io::{Cursor, Read, Write};

use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

use crate::charts::{Node, parse_xml, resolve};

/// Todas as partes do pacote, com o nome e o conteúdo.
pub fn read_parts(xlsx: &[u8]) -> Result<Vec<(String, Vec<u8>)>, String> {
    let fail = |e: &dyn std::fmt::Display| format!("arquivo inválido: {e}");
    let mut archive = ZipArchive::new(Cursor::new(xlsx)).map_err(|e| fail(&e))?;
    let mut parts = Vec::new();
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
    Ok(parts)
}

/// Monta o pacote de volta a partir das partes.
pub fn write_parts(parts: Vec<(String, Vec<u8>)>) -> Result<Vec<u8>, String> {
    let fail = |e: &dyn std::fmt::Display| format!("não foi possível gravar o arquivo: {e}");
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, bytes) in parts {
        writer.start_file(name, options).map_err(|e| fail(&e))?;
        writer.write_all(&bytes).map_err(|e| fail(&e))?;
    }
    Ok(writer.finish().map_err(|e| fail(&e))?.into_inner())
}

/// Tira uma parte de texto da lista.
pub fn take_text(parts: &mut Vec<(String, Vec<u8>)>, name: &str) -> Option<String> {
    parts.iter().position(|(n, _)| n == name).map(|i| String::from_utf8_lossy(&parts.remove(i).1).into_owned())
}

/// Em que parte do pacote está cada planilha: (posição em `sheet_names`, caminho).
pub fn sheet_parts(xlsx: &[u8], sheet_names: &[String]) -> Vec<(usize, String)> {
    let Ok(parts) = read_parts(xlsx) else { return Vec::new() };
    let find = |name: &str| parts.iter().find(|(n, _)| n == name).map(|(_, bytes)| bytes.as_slice());
    let Some(workbook) = find("xl/workbook.xml").and_then(parse_xml) else { return Vec::new() };
    let rels: Option<Node> = find("xl/_rels/workbook.xml.rels").and_then(parse_xml);
    let Some(sheets) = workbook.first("sheets") else { return Vec::new() };
    let mut found = Vec::new();
    for sheet in &sheets.children {
        let Some(index) = sheet.attr("name").and_then(|name| sheet_names.iter().position(|n| n == name)) else {
            continue;
        };
        let target = rels.as_ref().and_then(|rels| {
            rels.children.iter().find(|rel| rel.attr("Id") == sheet.attr("id")).and_then(|rel| rel.attr("Target"))
        });
        if let Some(target) = target {
            found.push((index, resolve("xl/workbook.xml", target)));
        }
    }
    found
}
