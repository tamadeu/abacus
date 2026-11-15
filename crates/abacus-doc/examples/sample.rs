//! Grava um `.docx` de exemplo: `cargo run -p abacus-doc --example sample -- saida.docx`.
use abacus_doc::*;

fn main() {
    let path = std::env::args().nth(1).expect("caminho de saída");
    let run = |t: &str, bold: bool| Run { text: t.into(), bold, ..Run::default() };
    let item = |kind, t: &str| Block::Paragraph(Paragraph { kind, ..Paragraph::text(t) });
    let doc = Document {
        blocks: vec![
            Block::Paragraph(Paragraph { kind: ParaKind::Heading(1), ..Paragraph::text("Título do documento") }),
            Block::Paragraph(Paragraph {
                runs: vec![run("Um parágrafo com ", false), run("negrito", true), run(" no meio.", false)],
                ..Paragraph::default()
            }),
            item(ParaKind::Bullet, "primeiro marcador"),
            item(ParaKind::Bullet, "segundo marcador"),
            item(ParaKind::Numbered, "passo um"),
            item(ParaKind::Numbered, "passo dois"),
        ],
    };
    std::fs::write(path, write_docx(&doc).unwrap()).unwrap();
}
