//! 把调用方的消息渲染为安全 HTML。这是唯一的内容安全边界：前端只接收这里的输出。

use std::collections::HashSet;
use std::sync::OnceLock;

use ammonia::Builder;
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd, html};
use syntect::html::{ClassStyle, ClassedHTMLGenerator};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;
use turndock_core::MessageFormat;

pub fn render(source: &str, format: MessageFormat) -> String {
    match format {
        MessageFormat::Markdown => render_markdown(source),
        MessageFormat::Plain => render_plain(source),
    }
}

fn render_markdown(source: &str) -> String {
    let parser = Parser::new_ext(source, options());
    let mut events = Vec::new();
    let mut code: Option<(String, String)> = None; // (语言, 内容)
    for ev in parser {
        match (&mut code, ev) {
            (None, Event::Start(Tag::CodeBlock(kind))) => {
                let lang = match kind {
                    CodeBlockKind::Fenced(info) => {
                        info.split([' ', ',', '{']).next().unwrap_or("").to_owned()
                    }
                    CodeBlockKind::Indented => String::new(),
                };
                code = Some((lang, String::new()));
            }
            (Some((_, buf)), Event::Text(t)) => buf.push_str(&t),
            (Some(_), Event::End(TagEnd::CodeBlock)) => {
                if let Some((lang, buf)) = code.take() {
                    events.push(Event::Html(code_block(&lang, &buf).into()));
                }
            }
            (Some(_), _) => {}
            (None, ev) => events.push(ev),
        }
    }
    let mut raw = String::with_capacity(source.len() * 2);
    html::push_html(&mut raw, events.into_iter());
    sanitizer().clean(&raw).to_string()
}

fn options() -> Options {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    opts
}

/// 单段 Markdown 的纯文本（用于标题）：只保留文字与行内代码，丢弃 HTML 与格式标记。
pub fn plain_text(source: &str) -> String {
    let mut out = String::new();
    // script/style 标签之间的文字不是给人看的内容。
    let mut hidden = false;
    for ev in Parser::new_ext(source, options()) {
        match ev {
            Event::InlineHtml(h) | Event::Html(h) => {
                let tag = h.trim_start().to_ascii_lowercase();
                if tag.starts_with("<script") || tag.starts_with("<style") {
                    hidden = true;
                } else if tag.starts_with("</script") || tag.starts_with("</style") {
                    hidden = false;
                }
            }
            Event::Text(_) | Event::Code(_) if hidden => {}
            Event::Text(t) | Event::Code(t) => out.push_str(&t),
            Event::SoftBreak | Event::HardBreak => out.push(' '),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 代码块：已知语言时用 syntect 生成带 `hl-` 前缀 class 的 span，配色由前端 token 决定。
fn code_block(lang: &str, code: &str) -> String {
    const MAX_HIGHLIGHT: usize = 64 * 1024;
    let label = escape(lang);
    let set = syntaxes();
    let syntax = (!lang.is_empty() && code.len() <= MAX_HIGHLIGHT)
        .then(|| set.find_syntax_by_token(lang))
        .flatten();
    let body = match syntax {
        Some(syntax) => {
            let mut g = ClassedHTMLGenerator::new_with_class_style(
                syntax,
                set,
                ClassStyle::SpacedPrefixed { prefix: "hl-" },
            );
            let ok = LinesWithEndings::from(code)
                .all(|line| g.parse_html_for_line_which_includes_newline(line).is_ok());
            if ok { g.finalize() } else { escape(code) }
        }
        None => escape(code),
    };
    if label.is_empty() {
        format!("<pre><code>{body}</code></pre>")
    } else {
        format!("<pre data-lang=\"{label}\"><code class=\"language-{label}\">{body}</code></pre>")
    }
}

/// 在后台加载语法集，避免第一条消息渲染时卡顿。
pub fn warm_up() {
    let _ = std::thread::Builder::new()
        .name("turndock-syntaxes".into())
        .spawn(|| {
            syntaxes();
        });
}

fn syntaxes() -> &'static SyntaxSet {
    static SET: OnceLock<SyntaxSet> = OnceLock::new();
    // bat 的扩展语法集：在 syntect 默认之外补上 TOML、TypeScript、Vue、Swift、Dockerfile 等。
    SET.get_or_init(two_face::syntax::extra_newlines)
}

fn render_plain(source: &str) -> String {
    let escaped = escape(&source.replace("\r\n", "\n"));
    escaped
        .split("\n\n")
        .filter(|p| !p.trim().is_empty())
        .map(|p| format!("<p>{}</p>", p.trim_matches('\n').replace('\n', "<br>")))
        .collect()
}

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

fn sanitizer() -> Builder<'static> {
    let mut b = Builder::default();
    // 不加载任何远程资源：去掉图片；链接只允许 http/https/mailto。
    b.rm_tags(["img"]);
    b.add_tags(["input"]);
    b.add_tag_attributes("input", ["type", "checked", "disabled"]);
    b.add_tag_attributes("code", ["class"]);
    b.add_tag_attributes("span", ["class"]);
    b.add_tag_attributes("pre", ["data-lang"]);
    b.add_tag_attribute_values("input", "type", ["checkbox"]);
    b.url_schemes(HashSet::from(["http", "https", "mailto"]));
    b.link_rel(Some("noopener noreferrer"));
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_scripts_event_handlers_and_js_links() {
        let html = render(
            "<img src=x onerror=alert(1)>\n\n<script>alert(2)</script>\n\n[a](javascript:alert(3)) <b onclick=\"x()\">b</b>",
            MessageFormat::Markdown,
        );
        assert!(!html.contains("onerror"), "{html}");
        assert!(!html.contains("<script"), "{html}");
        assert!(!html.contains("javascript:"), "{html}");
        assert!(!html.contains("onclick"), "{html}");
        assert!(!html.contains("<img"), "{html}");
    }

    #[test]
    fn keeps_common_markdown() {
        let html = render(
            "# 标题\n\n- [x] 完成\n\n```rust\nfn main() {}\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n[链接](https://example.com)",
            MessageFormat::Markdown,
        );
        assert!(html.contains("<h1>标题</h1>"), "{html}");
        assert!(html.contains("class=\"language-rust\""), "{html}");
        assert!(html.contains("<table>"), "{html}");
        assert!(html.contains("rel=\"noopener noreferrer\""), "{html}");
        assert!(html.contains("type=\"checkbox\""), "{html}");
    }

    #[test]
    fn code_blocks_are_highlighted_with_classes() {
        let html = render("```rust\nfn main() {}\n```", MessageFormat::Markdown);
        assert!(html.contains("data-lang=\"rust\""), "{html}");
        assert!(html.contains("hl-"), "{html}");
        let unknown = render("```nope\n<b>x</b>\n```", MessageFormat::Markdown);
        assert!(unknown.contains("&lt;b&gt;"), "{unknown}");
    }

    #[test]
    fn common_languages_are_highlighted() {
        for lang in [
            "toml",
            "ts",
            "tsx",
            "typescript",
            "vue",
            "swift",
            "dockerfile",
            "bash",
            "json",
            "yaml",
            "python",
            "go",
            "sql",
            "diff",
        ] {
            let html = render(
                &format!("```{lang}\nlet x = 1 // c\n```"),
                MessageFormat::Markdown,
            );
            assert!(html.contains("hl-"), "{lang} 没有高亮：{html}");
        }
    }

    #[test]
    fn plain_text_drops_html_and_marks() {
        assert_eq!(
            plain_text(
                "两组笔记 **37** 处重复，`a`？<img src=x onerror=alert(1)><script>alert(2)</script>"
            ),
            "两组笔记 37 处重复，a？"
        );
    }

    #[test]
    fn remote_markdown_images_are_not_loaded() {
        let html = render(
            "![x](https://tracker.example/p.png)",
            MessageFormat::Markdown,
        );
        assert!(!html.contains("tracker.example"), "{html}");
    }

    #[test]
    fn plain_text_is_escaped() {
        let html = render("a <b>\n第二行\n\n段落", MessageFormat::Plain);
        assert_eq!(html, "<p>a &lt;b&gt;<br>第二行</p><p>段落</p>");
    }
}
