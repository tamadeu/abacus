//! O documento de texto do Abacus: um modelo simples (parágrafos com títulos,
//! listas, alinhamento e trechos com negrito, itálico, sublinhado e tachado)
//! e a leitura e gravação de `.docx`.
//!
//! O que o modelo não conhece (imagens, tabelas, cabeçalhos, estilos
//! personalizados…) não é guardado ao abrir um arquivo.

mod docx;
mod model;
mod xml;

pub use docx::{read_docx, write_docx};
pub use model::{Align, Block, Document, ParaKind, Paragraph, Run};
