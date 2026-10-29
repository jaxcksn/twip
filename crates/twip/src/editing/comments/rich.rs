//! Comment bodies as formatted runs, to and from the blocks the document keeps.

use std::rc::Rc;

use crate::docx::resolve::Resolver;
use crate::docx::write::props::{Cx, Props};
use crate::docx::{Block, BreakKind, Inline, Paragraph, Run};

/// A comment's body: paragraphs of formatted runs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RichText {
    pub paragraphs: Vec<RichParagraph>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RichParagraph {
    pub runs: Vec<RichRun>,
}

/// Text in one formatting. A tab is `\t` and a line break `\n`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RichRun {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    /// The address the text links to.
    pub link: Option<String>,
}

impl RichRun {
    fn same_format(&self, o: &RichRun) -> bool {
        (self.bold, self.italic, self.underline, self.strike, &self.link) == (o.bold, o.italic, o.underline, o.strike, &o.link)
    }
}

impl RichText {
    /// Plain text: one run per line, each line a paragraph.
    pub fn plain(text: &str) -> RichText {
        let paragraphs = text
            .split('\n')
            .map(|l| l.strip_suffix('\r').unwrap_or(l))
            .map(|l| RichParagraph { runs: if l.is_empty() { Vec::new() } else { vec![RichRun { text: l.to_string(), ..RichRun::default() }] } })
            .collect();
        RichText { paragraphs }
    }

    /// The text, paragraphs separated by newlines.
    pub fn to_plain(&self) -> String {
        self.paragraphs.iter().map(|p| p.runs.iter().map(|r| r.text.as_str()).collect::<String>()).collect::<Vec<_>>().join("\n")
    }

    /// The body of a comment: paragraphs in the CommentText style when the document has it, runs with their
    /// formatting as direct properties, links as HYPERLINK fields.
    pub fn to_blocks(&self, res: &Resolver, cx: &Cx) -> Vec<Block> {
        let w = &cx.w;
        let mut pp = Props::parse(None, "pPr", cx);
        if res.styles.by_id.contains_key("CommentText") {
            pp.set("pStyle", format!("<{w}pStyle {w}val=\"CommentText\"/>"));
        }
        let para = res.para_from_xml(pp.xml().as_deref(), None, cx);
        let style = Rc::new(para.style);
        let plain = res.run_from_xml(&para.base, None, cx);
        let paragraphs: &[RichParagraph] = if self.paragraphs.is_empty() { &[RichParagraph { runs: Vec::new() }] } else { &self.paragraphs };
        paragraphs
            .iter()
            .map(|p| {
                let mut inlines = Vec::new();
                for r in p.runs.iter().filter(|r| !r.text.is_empty()) {
                    let mut rp = Props::parse(None, "rPr", cx);
                    for (on, name, xml) in [
                        (r.bold, "b", format!("<{w}b/>")),
                        (r.italic, "i", format!("<{w}i/>")),
                        (r.underline, "u", format!("<{w}u {w}val=\"single\"/>")),
                        (r.strike, "strike", format!("<{w}strike/>")),
                    ] {
                        if on {
                            rp.set(name, xml);
                        }
                    }
                    if r.link.is_some() && res.styles.by_id.contains_key("Hyperlink") {
                        rp.set("rStyle", format!("<{w}rStyle {w}val=\"Hyperlink\"/>"));
                    }
                    let run = res.run_from_xml(&para.base, rp.xml().as_deref(), cx);
                    match &r.link {
                        Some(address) => {
                            let code = format!(" HYPERLINK \"{}\" ", address.replace('"', ""));
                            inlines.extend(crate::docx::fields::field_inlines(&code, &r.text, &run));
                        }
                        None => inlines.extend(text_inlines(&r.text, &run)),
                    }
                }
                if inlines.is_empty() {
                    inlines.push(Inline::Text(String::new(), plain.clone()));
                }
                Block::Para(Paragraph {
                    style: style.clone(),
                    mark: plain.clone(),
                    list: None,
                    inlines,
                    src: None,
                    id: crate::docx::next_para_id(),
                    controls: Vec::new(),
                })
            })
            .collect()
    }

    /// A comment body's text and formatting: the paragraphs of its blocks (a table's cells' in order), each text's
    /// bold, italic, underline and strike, and the address of a HYPERLINK field around it.
    pub fn from_blocks(blocks: &[Block]) -> RichText {
        fn walk(blocks: &[Block], out: &mut Vec<RichParagraph>) {
            for b in blocks {
                match b {
                    Block::Para(p) => out.push(paragraph(p)),
                    Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).for_each(|c| walk(&c.blocks, out)),
                }
            }
        }
        let mut paragraphs = Vec::new();
        walk(blocks, &mut paragraphs);
        RichText { paragraphs }
    }
}

/// Text with tabs and line breaks as their inlines.
fn text_inlines(text: &str, run: &Run) -> Vec<Inline> {
    let mut out = Vec::new();
    for (k, line) in text.split('\n').enumerate() {
        if k > 0 {
            out.push(Inline::Break(BreakKind::Line, run.clone()));
        }
        for (j, seg) in line.split('\t').enumerate() {
            if j > 0 {
                out.push(Inline::Tab(run.clone()));
            }
            if !seg.is_empty() {
                out.push(Inline::Text(seg.to_string(), run.clone()));
            }
        }
    }
    out
}

fn paragraph(p: &Paragraph) -> RichParagraph {
    use crate::docx::fields;
    // Each inline's link: the address of the HYPERLINK field whose result holds it.
    let mut link: Vec<Option<String>> = vec![None; p.inlines.len()];
    let mut code = vec![false; p.inlines.len()];
    for s in fields::spans(&p.inlines) {
        code[s.begin..s.code_end().min(p.inlines.len())].iter_mut().for_each(|c| *c = true);
        let parsed = fields::parse(&fields::code_text(&p.inlines, s));
        if parsed.name == "HYPERLINK"
            && let (Some(address), Some(sep)) = (parsed.args.first(), s.sep)
        {
            link[sep + 1..s.end.min(p.inlines.len())].iter_mut().for_each(|l| *l = Some(address.clone()));
        }
    }
    let mut runs: Vec<RichRun> = Vec::new();
    for (k, i) in p.inlines.iter().enumerate() {
        let (text, r) = match i {
            Inline::Text(t, r) => (t.as_str(), r),
            Inline::Tab(r) => ("\t", r),
            Inline::Break(BreakKind::Line, r) => ("\n", r),
            _ => continue,
        };
        if code[k] || text.is_empty() || r.hidden {
            continue;
        }
        let run = RichRun {
            text: text.to_string(),
            bold: r.bold,
            italic: r.italic,
            underline: r.underline.as_deref().is_some_and(|u| u != "none"),
            strike: r.strike,
            link: link[k].clone(),
        };
        match runs.last_mut() {
            Some(last) if last.same_format(&run) => last.text.push_str(&run.text),
            _ => runs.push(run),
        }
    }
    RichParagraph { runs }
}
