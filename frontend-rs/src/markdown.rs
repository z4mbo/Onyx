use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd, html};

use crate::highlight;

pub fn render(source: &str) -> String {
    let parser = Parser::new_ext(
        source,
        Options::ENABLE_TABLES
            | Options::ENABLE_STRIKETHROUGH
            | Options::ENABLE_TASKLISTS
            | Options::ENABLE_FOOTNOTES,
    );

    // Fenced blocks are collected and re-emitted as pre-rendered HTML so the
    // tokenizer can style them. Everything it emits is escaped by the
    // tokenizer itself, and provider-supplied HTML stays inert as text.
    let mut events = Vec::new();
    let mut code_block = None::<(Option<String>, String)>;
    for event in parser {
        match event {
            Event::Start(Tag::CodeBlock(kind)) => {
                let hint = match kind {
                    CodeBlockKind::Fenced(info) => info
                        .split_whitespace()
                        .next()
                        .filter(|hint| !hint.is_empty())
                        .map(str::to_owned),
                    CodeBlockKind::Indented => None,
                };
                code_block = Some((hint, String::new()));
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some((hint, body)) = code_block.take() {
                    events.push(Event::Html(
                        highlight::code_block_html(&body, hint.as_deref()).into(),
                    ));
                }
            }
            Event::Text(value) | Event::Html(value) | Event::InlineHtml(value)
                if code_block.is_some() =>
            {
                if let Some((_, body)) = code_block.as_mut() {
                    body.push_str(&value);
                }
            }
            Event::Html(value) | Event::InlineHtml(value) => events.push(Event::Text(value)),
            other => events.push(other),
        }
    }
    if let Some((hint, body)) = code_block.take() {
        events.push(Event::Html(
            highlight::code_block_html(&body, hint.as_deref()).into(),
        ));
    }

    let mut output = String::new();
    html::push_html(&mut output, events.into_iter());
    output
}

#[cfg(test)]
mod tests {
    use super::render;

    #[test]
    fn raw_html_is_inert() {
        let rendered = render("<script>alert(1)</script>");
        assert!(!rendered.contains("<script>"));
        assert!(rendered.contains("&lt;script&gt;"));
    }

    #[test]
    fn fenced_code_is_highlighted() {
        let rendered = render("```bash\ncargo test --workspace\n```");
        assert!(rendered.contains("language-bash"));
        assert!(rendered.contains("syn-prop"));
    }

    #[test]
    fn unlabelled_shell_blocks_are_detected() {
        let rendered = render("```\ngit status --short\n```");
        assert!(rendered.contains("language-bash"));
    }

    #[test]
    fn html_inside_a_code_block_stays_text() {
        // Angle brackets may be split across highlight spans; what matters is
        // that no live tag survives.
        let rendered = render("```html\n<script>alert(1)</script>\n```");
        assert!(!rendered.contains("<script>"));
        assert!(!rendered.contains("</script>"));
        assert!(rendered.contains("&lt;"));
        assert!(rendered.contains("script"));
    }

    #[test]
    fn prose_around_code_still_renders() {
        let rendered = render("Run this:\n\n```sh\nls -la\n```\n\nDone.");
        assert!(rendered.contains("<p>Run this:</p>"));
        assert!(rendered.contains("<p>Done.</p>"));
        assert!(rendered.contains("<pre><code"));
    }
}
