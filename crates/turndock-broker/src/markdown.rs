//! 把调用方的消息渲染为安全 HTML。这是唯一的内容安全边界：前端只接收这里的输出。

use std::collections::HashSet;

use ammonia::Builder;
use pulldown_cmark::{Options, Parser, html};
use turndock_core::MessageFormat;

pub fn render(source: &str, format: MessageFormat) -> String {
    match format {
        MessageFormat::Markdown => render_markdown(source),
        MessageFormat::Plain => render_plain(source),
    }
}

fn render_markdown(source: &str) -> String {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    let parser = Parser::new_ext(source, opts);
    let mut raw = String::with_capacity(source.len() * 3 / 2);
    html::push_html(&mut raw, parser);
    sanitizer().clean(&raw).to_string()
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
