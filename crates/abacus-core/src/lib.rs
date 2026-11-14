//! Modelo de dados do Ábaco.
//!
//! Este crate não depende de interface nem de formato de arquivo: guarda o
//! que o usuário digitou em cada célula e define os tipos que o motor de
//! fórmulas (`abacus-formula`) e as cascas (desktop, web) compartilham.

mod cell_ref;
mod number;
mod sheet;
mod value;

pub use cell_ref::{CellRef, MAX_COLS, MAX_ROWS};
pub use number::{format_number, parse_number};
pub use sheet::{Sheet, Workbook};
pub use value::{CellError, Value};
