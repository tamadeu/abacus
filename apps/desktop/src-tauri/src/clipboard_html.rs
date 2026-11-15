//! A seleção copiada como tabela HTML, para colar em outros programas (Word,
//! Excel, e-mail, navegador) com a formatação que a grade mostra.

use crate::engine::{Align, CellKind, CellView, Engine, Range, VAlign};

/// Acima disso só vai o texto: uma tabela de milhões de células travaria o programa.
const MAX_CELLS: u64 = 40_000;

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\n' => out.push_str("<br>"),
            c => out.push(c),
        }
    }
    out
}

/// As propriedades CSS de uma célula.
fn css(view: &CellView) -> String {
    let mut css = String::from("padding:2px 4px;");
    let Some(style) = &view.style else {
        // Sem estilo, número fica à direita, como na grade.
        if view.kind == CellKind::Number {
            css.push_str("text-align:right;");
        }
        return css;
    };
    if style.bold {
        css.push_str("font-weight:bold;");
    }
    if style.italic {
        css.push_str("font-style:italic;");
    }
    match (style.underline, style.strike) {
        (true, true) => css.push_str("text-decoration:underline line-through;"),
        (true, false) => css.push_str("text-decoration:underline;"),
        (false, true) => css.push_str("text-decoration:line-through;"),
        _ => {}
    }
    if let Some(color) = &style.color {
        css.push_str(&format!("color:{color};"));
    }
    if let Some(fill) = &style.fill {
        css.push_str(&format!("background-color:{fill};"));
    }
    if let Some(size) = style.size {
        // O tamanho é a porcentagem do padrão de 11 pt.
        css.push_str(&format!("font-size:{:.1}pt;", f64::from(size) * 11.0 / 100.0));
    }
    match style.align {
        Some(Align::Left) => css.push_str("text-align:left;"),
        Some(Align::Center) => css.push_str("text-align:center;"),
        Some(Align::Right) => css.push_str("text-align:right;"),
        None if view.kind == CellKind::Number => css.push_str("text-align:right;"),
        None => {}
    }
    match style.valign {
        Some(VAlign::Top) => css.push_str("vertical-align:top;"),
        Some(VAlign::Center) => css.push_str("vertical-align:middle;"),
        None => css.push_str("vertical-align:bottom;"),
    }
    if !style.wrap {
        css.push_str("white-space:nowrap;");
    }
    for (side, border) in ["top", "right", "bottom", "left"].iter().zip(&style.borders) {
        if let Some(line) = border {
            let kind = if line.dashed { "dashed" } else { "solid" };
            css.push_str(&format!("border-{side}:{}px {kind} {};", line.width, line.color));
        }
    }
    css
}

impl Engine {
    /// O intervalo como tabela HTML, ou `None` se for grande demais.
    pub fn copy_html(&self, sheet: u32, range: Range) -> Result<Option<String>, String> {
        if u64::from(range.cols) * u64::from(range.rows) > MAX_CELLS {
            return Ok(None);
        }
        let mut html = String::from(
            "<table style=\"border-collapse:collapse;font-family:Calibri,Arial,sans-serif;font-size:11pt\">",
        );
        for row in range.row..range.row + range.rows {
            html.push_str("<tr>");
            for col in range.col..range.col + range.cols {
                let view = self.view_of_cell(sheet, col, row)?;
                let (css, text) = match &view {
                    Some(view) => (css(view), escape(&view.display)),
                    None => ("padding:2px 4px;".to_string(), String::new()),
                };
                html.push_str(&format!("<td style=\"{css}\">{text}</td>"));
            }
            html.push_str("</tr>");
        }
        html.push_str("</table>");
        Ok(Some(html))
    }
}

#[cfg(test)]
mod tests {
    use crate::engine::{Engine, Range, StyleProperty};

    #[test]
    fn copies_cells_as_a_styled_html_table() {
        let mut e = Engine::new().unwrap();
        e.set_cell(0, 0, 0, "Total & <b>").unwrap();
        e.set_cell(0, 1, 0, "10").unwrap();
        e.format(0, Range { col: 0, row: 0, cols: 1, rows: 1 }, StyleProperty::Bold, "true").unwrap();
        e.format(0, Range { col: 0, row: 0, cols: 1, rows: 1 }, StyleProperty::Fill, "#FFEE00").unwrap();
        let html = e.copy_html(0, Range { col: 0, row: 0, cols: 2, rows: 1 }).unwrap().unwrap();
        assert!(html.contains("Total &amp; &lt;b&gt;"), "{html}");
        assert!(html.contains("font-weight:bold"), "{html}");
        assert!(html.contains("background-color:#FFEE00"), "{html}");
        // O número, sem estilo, fica à direita.
        assert!(html.contains("text-align:right;\">10<"), "{html}");
    }

    #[test]
    fn huge_ranges_stay_as_text() {
        let e = Engine::new().unwrap();
        let all = Range { col: 0, row: 0, cols: 16_384, rows: 1_048_576 };
        assert!(e.copy_html(0, all).unwrap().is_none());
    }
}
