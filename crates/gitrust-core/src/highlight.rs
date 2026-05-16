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
///
/// Markdown is special-cased: tree-sitter-md's block→inline injection
/// is wired but inline events don't surface through the upstream
/// `tree-sitter-highlight` we use. So we run the inline grammar over
/// the whole document in a second pass and merge the two token streams
/// per line — inline classes win where they're non-empty, the block
/// pass owns everything else (headings, code fences, list bullets).
pub fn highlight_per_line(bytes: &[u8], lang: &str) -> Option<Vec<Vec<Token>>> {
    let block = highlight_one_pass(bytes, lang)?;
    if lang == "markdown"
        && let Some(inline) = highlight_one_pass(bytes, "markdown_inline")
    {
        return Some(merge_md_passes(block, inline));
    }
    Some(block)
}

fn highlight_one_pass(bytes: &[u8], lang: &str) -> Option<Vec<Vec<Token>>> {
    let cfg = config_for(lang)?;
    let mut highlighter = Highlighter::new();
    let events = highlighter
        .highlight(cfg, bytes, None, {
            #[allow(clippy::redundant_closure)]
            |name| config_for(name)
        })
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

/// Walk block and inline tokens for the same document in lockstep
/// (char by char) and pick the non-empty class. Both passes tokenize
/// the same bytes, so concatenating the text of each line should
/// produce identical strings — when they don't (parser disagreement
/// on edge cases) the block line wins as a safe fallback.
fn merge_md_passes(block: Vec<Vec<Token>>, inline: Vec<Vec<Token>>) -> Vec<Vec<Token>> {
    let n = block.len().min(inline.len());
    let mut out = Vec::with_capacity(block.len());
    for i in 0..n {
        out.push(merge_md_line(&block[i], &inline[i]));
    }
    // Trailing block lines (block had more lines than inline parsed) pass through.
    for line in block.into_iter().skip(n) {
        out.push(line);
    }
    out
}

fn merge_md_line(block: &[Token], inline: &[Token]) -> Vec<Token> {
    let block_chars: Vec<(char, &str)> = block
        .iter()
        .flat_map(|t| t.text.chars().map(move |c| (c, t.class.as_str())))
        .collect();
    let inline_chars: Vec<(char, &str)> = inline
        .iter()
        .flat_map(|t| t.text.chars().map(move |c| (c, t.class.as_str())))
        .collect();

    if block_chars.len() != inline_chars.len() {
        return block.to_vec();
    }

    let mut merged: Vec<Token> = Vec::new();
    let mut cur_text = String::new();
    let mut cur_class: Option<String> = None;
    for (i, (ch, block_class)) in block_chars.iter().enumerate() {
        let inline_class = inline_chars[i].1;
        let eff = if !inline_class.is_empty() {
            inline_class
        } else {
            block_class
        };
        match &cur_class {
            Some(c) if c == eff => cur_text.push(*ch),
            _ => {
                if let Some(c) = cur_class.take() {
                    merged.push(Token {
                        text: std::mem::take(&mut cur_text),
                        class: c,
                    });
                }
                cur_class = Some(eff.to_string());
                cur_text.push(*ch);
            }
        }
    }
    if let Some(c) = cur_class {
        merged.push(Token {
            text: cur_text,
            class: c,
        });
    }
    merged
}
