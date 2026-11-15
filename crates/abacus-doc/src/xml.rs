//! Um DOM mínimo, o bastante para ler as partes de um `.docx`.

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

#[derive(Debug, Default)]
pub struct Node {
    /// Nome local, sem o prefixo (`w:p` vira `p`).
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Node>,
    pub text: String,
}

impl Node {
    pub fn attr(&self, key: &str) -> Option<&str> {
        self.attrs.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }

    pub fn first(&self, name: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.name == name)
    }
}

pub fn parse(bytes: &[u8]) -> Option<Node> {
    let mut reader = Reader::from_reader(bytes);
    let mut stack = vec![Node::default()];
    let make = |element: &BytesStart| {
        let mut node = Node { name: element.local_name().as_ref().to_string(), ..Node::default() };
        for attr in element.attributes().flatten() {
            let key = attr.key.local_name().as_ref().to_string();
            let value =
                attr.normalized_value(quick_xml::XmlVersion::default()).map(|v| v.into_owned()).unwrap_or_default();
            node.attrs.push((key, value));
        }
        node
    };
    loop {
        match reader.read_event().ok()? {
            Event::Start(element) => stack.push(make(&element)),
            Event::Empty(element) => stack.last_mut()?.children.push(make(&element)),
            Event::End(_) => {
                let node = stack.pop()?;
                stack.last_mut()?.children.push(node);
            }
            Event::Text(text) => stack.last_mut()?.text.push_str(&text.xml10_content()),
            Event::GeneralRef(entity) => {
                let decoded = match entity.as_ref() {
                    "amp" => Some('&'),
                    "lt" => Some('<'),
                    "gt" => Some('>'),
                    "quot" => Some('"'),
                    "apos" => Some('\''),
                    _ => entity.resolve_char_ref().ok().flatten(),
                };
                if let Some(c) = decoded {
                    stack.last_mut()?.text.push(c);
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    let mut root = stack.pop()?;
    if !stack.is_empty() || root.children.len() != 1 {
        return None;
    }
    root.children.pop()
}

/// Escapa texto para pôr entre marcas XML.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            // Caracteres de controle não cabem em XML 1.0.
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => {}
            c => out.push(c),
        }
    }
    out
}
