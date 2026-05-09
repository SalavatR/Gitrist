use std::path::Path;
use std::sync::OnceLock;

use tree_sitter::Language;
use tree_sitter_highlight::{HighlightConfiguration, HighlightEvent, Highlighter};

pub use gitrust_types::Token;

const HIGHLIGHT_NAMES: &[&str] = &[
    "attribute",
    "comment",
    "constant",
    "constant.builtin",
    "constructor",
    "function",
    "function.builtin",
    "function.macro",
    "function.method",
    "keyword",
    "keyword.control",
    "keyword.directive",
    "keyword.function",
    "label",
    "module",
    "namespace",
    "number",
    "operator",
    "property",
    "punctuation",
    "punctuation.bracket",
    "punctuation.delimiter",
    "punctuation.special",
    "string",
    "string.escape",
    "string.special",
    "tag",
    "text.emphasis",
    "text.literal",
    "text.reference",
    "text.strong",
    "text.title",
    "text.uri",
    "type",
    "type.builtin",
    "variable",
    "variable.builtin",
    "variable.parameter",
];

pub fn detect_language(path: &str) -> Option<&'static str> {
    let ext = Path::new(path).extension()?.to_str()?;
    Some(match ext.to_ascii_lowercase().as_str() {
        "rs" => "rust",
        "json" => "json",
        "html" | "htm" => "html",
        "css" => "css",
        "ts" => "typescript",
        "tsx" => "tsx",
        "js" | "jsx" | "mjs" | "cjs" => "javascript",
        "py" => "python",
        "toml" => "toml",
        "lua" => "lua",
        "md" | "markdown" => "markdown",
        _ => return None,
    })
}

fn config_for(lang: &str) -> Option<&'static HighlightConfiguration> {
    macro_rules! lang_cfg {
        ($cell:ident, $name:literal, $lf:expr, $hl:expr, $inj:expr, $loc:expr) => {{
            static $cell: OnceLock<Option<HighlightConfiguration>> = OnceLock::new();
            $cell
                .get_or_init(|| {
                    let language: Language = $lf.into();
                    match HighlightConfiguration::new(language, $name, $hl, $inj, $loc) {
                        Ok(mut cfg) => {
                            cfg.configure(HIGHLIGHT_NAMES);
                            Some(cfg)
                        }
                        Err(_) => None,
                    }
                })
                .as_ref()
        }};
    }

    match lang {
        "rust" => lang_cfg!(
            RUST,
            "rust",
            tree_sitter_rust::LANGUAGE,
            tree_sitter_rust::HIGHLIGHTS_QUERY,
            tree_sitter_rust::INJECTIONS_QUERY,
            ""
        ),
        "json" => lang_cfg!(
            JSON,
            "json",
            tree_sitter_json::LANGUAGE,
            tree_sitter_json::HIGHLIGHTS_QUERY,
            "",
            ""
        ),
        "html" => lang_cfg!(
            HTML,
            "html",
            tree_sitter_html::LANGUAGE,
            tree_sitter_html::HIGHLIGHTS_QUERY,
            tree_sitter_html::INJECTIONS_QUERY,
            ""
        ),
        "css" => lang_cfg!(
            CSS,
            "css",
            tree_sitter_css::LANGUAGE,
            tree_sitter_css::HIGHLIGHTS_QUERY,
            "",
            ""
        ),
        "typescript" => lang_cfg!(
            TS,
            "typescript",
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT,
            tree_sitter_typescript::HIGHLIGHTS_QUERY,
            "",
            tree_sitter_typescript::LOCALS_QUERY
        ),
        "tsx" => lang_cfg!(
            TSX,
            "tsx",
            tree_sitter_typescript::LANGUAGE_TSX,
            tree_sitter_typescript::HIGHLIGHTS_QUERY,
            "",
            tree_sitter_typescript::LOCALS_QUERY
        ),
        "javascript" => lang_cfg!(
            JS,
            "javascript",
            tree_sitter_javascript::LANGUAGE,
            tree_sitter_javascript::HIGHLIGHT_QUERY,
            tree_sitter_javascript::INJECTIONS_QUERY,
            tree_sitter_javascript::LOCALS_QUERY
        ),
        "python" => lang_cfg!(
            PY,
            "python",
            tree_sitter_python::LANGUAGE,
            tree_sitter_python::HIGHLIGHTS_QUERY,
            "",
            ""
        ),
        "toml" => lang_cfg!(
            TOML,
            "toml",
            tree_sitter_toml_ng::LANGUAGE,
            tree_sitter_toml_ng::HIGHLIGHTS_QUERY,
            "",
            ""
        ),
        "lua" => lang_cfg!(
            LUA,
            "lua",
            tree_sitter_lua::LANGUAGE,
            tree_sitter_lua::HIGHLIGHTS_QUERY,
            tree_sitter_lua::INJECTIONS_QUERY,
            tree_sitter_lua::LOCALS_QUERY
        ),
        "markdown" => lang_cfg!(
            MD_BLOCK,
            "markdown",
            tree_sitter_md::LANGUAGE,
            tree_sitter_md::HIGHLIGHT_QUERY_BLOCK,
            tree_sitter_md::INJECTION_QUERY_BLOCK,
            ""
        ),
        "markdown_inline" => lang_cfg!(
            MD_INLINE,
            "markdown_inline",
            tree_sitter_md::INLINE_LANGUAGE,
            tree_sitter_md::HIGHLIGHT_QUERY_INLINE,
            tree_sitter_md::INJECTION_QUERY_INLINE,
            ""
        ),
        _ => None,
    }
}

/// Highlight `bytes` and return tokens grouped by line.
/// Returns `None` when the language isn't supported or the highlighter errors.
pub fn highlight_per_line(bytes: &[u8], lang: &str) -> Option<Vec<Vec<Token>>> {
    let cfg = config_for(lang)?;
    let mut highlighter = Highlighter::new();
    let events = highlighter
        .highlight(cfg, bytes, None, |injected_name| config_for(injected_name))
        .ok()?;

    let mut lines: Vec<Vec<Token>> = vec![Vec::new()];
    let mut stack: Vec<&'static str> = Vec::new();

    for event in events {
        let Ok(event) = event else { return None };
        match event {
            HighlightEvent::Source { start, end } => {
                let class = stack.last().copied().unwrap_or("");
                let mut cursor = start;
                while cursor < end {
                    let nl_offset = bytes[cursor..end].iter().position(|&b| b == b'\n');
                    match nl_offset {
                        Some(n) => {
                            let chunk_end = cursor + n;
                            if chunk_end > cursor {
                                let text =
                                    String::from_utf8_lossy(&bytes[cursor..chunk_end]).into_owned();
                                lines.last_mut().unwrap().push(Token {
                                    text,
                                    class: class.into(),
                                });
                            }
                            lines.push(Vec::new());
                            cursor = chunk_end + 1;
                        }
                        None => {
                            let text = String::from_utf8_lossy(&bytes[cursor..end]).into_owned();
                            if !text.is_empty() {
                                lines.last_mut().unwrap().push(Token {
                                    text,
                                    class: class.into(),
                                });
                            }
                            cursor = end;
                        }
                    }
                }
            }
            HighlightEvent::HighlightStart(s) => {
                if let Some(name) = HIGHLIGHT_NAMES.get(s.0).copied() {
                    stack.push(name);
                }
            }
            HighlightEvent::HighlightEnd => {
                stack.pop();
            }
        }
    }

    Some(lines)
}
