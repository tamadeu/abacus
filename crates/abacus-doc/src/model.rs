use serde::{Deserialize, Serialize};

/// Um documento: uma sequência de parágrafos.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Document {
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Block {
    Paragraph(Paragraph),
}

/// O que um parágrafo é.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ParaKind {
    #[default]
    Normal,
    /// Título de nível 1 a 6.
    Heading(u8),
    /// Item de lista com marcadores.
    Bullet,
    /// Item de lista numerada.
    Numbered,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
    Justify,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Paragraph {
    pub kind: ParaKind,
    pub align: Align,
    pub runs: Vec<Run>,
}

/// Um trecho de texto com a mesma formatação. `\n` é quebra de linha e `\t` é tabulação.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Run {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
}

impl Paragraph {
    /// Um parágrafo de texto simples.
    pub fn text(text: &str) -> Paragraph {
        Paragraph { runs: vec![Run { text: text.into(), ..Run::default() }], ..Paragraph::default() }
    }

    /// O texto do parágrafo, sem formatação.
    pub fn plain(&self) -> String {
        self.runs.iter().map(|run| run.text.as_str()).collect()
    }
}

impl Document {
    /// O texto de todos os parágrafos, um por linha.
    pub fn plain(&self) -> String {
        let lines: Vec<String> = self.blocks.iter().map(|Block::Paragraph(p)| p.plain()).collect();
        lines.join("\n")
    }
}
