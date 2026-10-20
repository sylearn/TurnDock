//! 把标准输入的 Markdown 渲染为净化后的 HTML，用于检查渲染效果与生成预览示例。
//! 用法：cargo run -q -p turndock-broker --example render < message.md

use std::io::Read;

fn main() -> std::io::Result<()> {
    let mut src = String::new();
    std::io::stdin().read_to_string(&mut src)?;
    print!(
        "{}",
        turndock_broker::markdown::render(&src, turndock_core::MessageFormat::Markdown)
    );
    Ok(())
}
