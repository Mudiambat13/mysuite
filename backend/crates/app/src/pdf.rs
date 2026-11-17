//! Générateur PDF minimal (texte, Helvetica intégrée, A4) — sans dépendance externe.
//! Suffisant pour des bulletins : pas d'images ni de polices embarquées.

const PAGE_W: f32 = 595.0;
const PAGE_H: f32 = 842.0;

#[derive(Clone, Copy, PartialEq)]
pub enum Font {
    Regular,
    Bold,
}

struct Op {
    x: f32,
    y: f32,
    size: f32,
    font: Font,
    text: String,
}

#[derive(Default)]
pub struct Document {
    pages: Vec<Vec<Op>>,
}

/// Encode en WinAnsi (cp1252) : accents français et €. Les autres caractères deviennent `?`.
fn winansi(c: char) -> u8 {
    match c {
        '€' => 0x80,
        '’' => 0x92,
        '—' => 0x97,
        '–' => 0x96,
        '…' => 0x85,
        '\u{a0}' | '\u{202f}' => b' ',
        c if (c as u32) < 0x100 => c as u8,
        _ => b'?',
    }
}

/// Largeur approximative d'une chaîne en Helvetica (unités de 1/1000 em).
pub fn text_width(text: &str, size: f32, font: Font) -> f32 {
    let w: f32 = text
        .chars()
        .map(|c| match c {
            '0'..='9' => 556.0,
            ' ' | ',' | '.' | ':' | ';' | '|' | 'i' | 'j' | 'l' | 'I' | '\'' => 278.0,
            '-' | '(' | ')' | 'f' | 't' | 'r' => 333.0,
            'm' | 'M' | 'W' | 'w' | '%' => 833.0,
            'A'..='Z' => 667.0,
            _ => 556.0,
        })
        .sum();
    w * size / 1000.0 * if font == Font::Bold { 1.05 } else { 1.0 }
}

fn escape(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    for c in text.chars() {
        let b = winansi(c);
        if matches!(b, b'(' | b')' | b'\\') {
            out.push(b'\\');
        }
        // Les caractères de contrôle n'ont rien à faire dans un texte affiché.
        out.push(if b < 0x20 { b' ' } else { b });
    }
    out
}

impl Document {
    pub fn new() -> Self {
        Self {
            pages: vec![Vec::new()],
        }
    }

    pub fn new_page(&mut self) {
        self.pages.push(Vec::new());
    }

    pub fn text(&mut self, x: f32, y: f32, size: f32, font: Font, text: &str) {
        if let Some(p) = self.pages.last_mut() {
            p.push(Op {
                x,
                y,
                size,
                font,
                text: text.to_string(),
            });
        }
    }

    pub fn text_right(&mut self, right: f32, y: f32, size: f32, font: Font, text: &str) {
        let x = right - text_width(text, size, font);
        self.text(x, y, size, font, text);
    }

    pub fn page_size() -> (f32, f32) {
        (PAGE_W, PAGE_H)
    }

    pub fn finish(self) -> Vec<u8> {
        // Objets : 1 catalogue, 2 pages, 3 police normale, 4 police grasse, puis (page, contenu) par page.
        let n = self.pages.len();
        let mut objects: Vec<Vec<u8>> = Vec::new();
        objects.push(b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
        let kids: Vec<String> = (0..n).map(|i| format!("{} 0 R", 5 + i * 2)).collect();
        objects.push(
            format!("<< /Type /Pages /Kids [{}] /Count {} >>", kids.join(" "), n).into_bytes(),
        );
        objects.push(
            b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"
                .to_vec(),
        );
        objects.push(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>".to_vec());
        for (i, ops) in self.pages.iter().enumerate() {
            let content_id = 6 + i * 2;
            objects.push(
                format!(
                    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {PAGE_W} {PAGE_H}] /Resources << /Font << /F1 3 0 R /F2 4 0 R >> >> /Contents {content_id} 0 R >>"
                )
                .into_bytes(),
            );
            let mut stream: Vec<u8> = Vec::new();
            for op in ops {
                let f = if op.font == Font::Bold { "F2" } else { "F1" };
                stream.extend_from_slice(
                    format!("BT /{f} {} Tf {:.2} {:.2} Td (", op.size, op.x, op.y).as_bytes(),
                );
                stream.extend_from_slice(&escape(&op.text));
                stream.extend_from_slice(b") Tj ET\n");
            }
            let mut obj = format!("<< /Length {} >>\nstream\n", stream.len()).into_bytes();
            obj.extend_from_slice(&stream);
            obj.extend_from_slice(b"endstream");
            objects.push(obj);
        }

        let mut out: Vec<u8> = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for (i, obj) in objects.iter().enumerate() {
            offsets.push(out.len());
            out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
            out.extend_from_slice(obj);
            out.extend_from_slice(b"\nendobj\n");
        }
        let xref = out.len();
        out.extend_from_slice(
            format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
        );
        for off in offsets {
            out.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
        }
        out.extend_from_slice(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
                objects.len() + 1
            )
            .as_bytes(),
        );
        out
    }
}

/// Formate un montant décimal exact : espaces pour les milliers, virgule décimale (ex. `1 234,50`).
pub fn format_amount(raw: &str) -> String {
    let (neg, rest) = match raw.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, raw),
    };
    let (int, frac) = rest.split_once('.').unwrap_or((rest, ""));
    let mut grouped = String::new();
    for (i, ch) in int.chars().enumerate() {
        if i > 0 && (int.len() - i) % 3 == 0 {
            grouped.push(' ');
        }
        grouped.push(ch);
    }
    let sign = if neg { "-" } else { "" };
    if frac.is_empty() {
        format!("{sign}{grouped}")
    } else {
        format!("{sign}{grouped},{frac}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amounts_are_grouped() {
        assert_eq!(format_amount("1234567.50"), "1 234 567,50");
        assert_eq!(format_amount("800.00"), "800,00");
        assert_eq!(format_amount("-1000.5"), "-1 000,5");
        assert_eq!(format_amount("0"), "0");
    }

    #[test]
    fn pdf_is_structurally_valid() {
        let mut d = Document::new();
        d.text(50.0, 800.0, 12.0, Font::Bold, "Bulletin (test) — é à € \\");
        d.new_page();
        d.text_right(500.0, 700.0, 10.0, Font::Regular, "1 000,00");
        let bytes = d.finish();
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.starts_with("%PDF-1.4"));
        assert!(text.contains("/Count 2"));
        assert!(text.trim_end().ends_with("%%EOF"));
        // Les parenthèses et antislashs du texte sont échappés.
        assert!(text.contains("\\(test\\)"));
    }
}
