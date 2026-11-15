//! Mostra o que o Abacus entende de um `.docx`: `cargo run -p abacus-doc --example dump -- arquivo.docx`.
fn main() {
    let path = std::env::args().nth(1).expect("caminho do arquivo");
    let doc = abacus_doc::read_docx(&std::fs::read(path).unwrap()).unwrap();
    println!("{doc:#?}");
}
