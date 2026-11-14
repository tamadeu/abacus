//! Motor de fórmulas do Ábaco.
//!
//! [`parse`] transforma o texto de uma fórmula em uma árvore ([`Expr`]) e
//! [`Evaluator`] calcula o valor de células, seguindo as referências entre
//! elas e detectando ciclos.
//!
//! Sintaxe aceita hoje, no padrão do Excel em português: números com vírgula
//! decimal, `+ - * / ^`, parênteses, referências (`B3`), intervalos
//! (`B3:B6`) e funções com argumentos separados por `;`.

mod eval;
mod parser;

pub use eval::{CellSource, Evaluator, evaluate};
pub use parser::{BinaryOp, Expr, ParseError, parse};
