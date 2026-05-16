//! File-diff rendering: unified gutter view, side-by-side view, hunks,
//! per-line tokens. `render_line_content` is shared with the blob viewer
//! in `main_panel`, the rest is private.

use dioxus::prelude::*;
use gitrust_types::{DiffHunk, DiffLine, FileDiff, Token};

/// Files with more than this many diff lines start collapsed. Tunable.
const AUTO_COLLAPSE_LINES: usize = 300;

pub(crate) fn render_file_diff(f: FileDiff, side_by_side: bool) -> Element {
    let path = f.path.clone();
    let old_path = f.old_path.clone();
    let kind = f.kind.clone();
    let is_binary = f.is_binary;
    let hunks = f.hunks.clone();
    let mut adds = 0usize;
    let mut dels = 0usize;
    let mut total_lines = 0usize;
    for h in &hunks {
        for l in &h.lines {
            total_lines += 1;
            match l.kind.as_str() {
                "add" => adds += 1,
                "del" => dels += 1,
                _ => {}
            }
        }
    }
    let no_hunks = hunks.is_empty();
    // Auto-collapse: huge diffs and binary/renamed files.
    let auto_open = !is_binary && !no_hunks && total_lines <= AUTO_COLLAPSE_LINES;
    rsx! {
        details { class: "file-diff", open: auto_open,
            summary { class: "file-header",
                span { class: "disclosure" }
                span { class: "kind kind-{kind}", "{kind}" }
                if let Some(op) = old_path {
                    code { class: "path old-path", "{op}" }
                    span { class: "rename-arrow", "→" }
                }
                code { class: "path", "{path}" }
                span { class: "stats",
                    span { class: "add-stat", "+{adds}" }
                    " "
                    span { class: "del-stat", "−{dels}" }
                }
            }
            if is_binary {
                div { class: "binary-note", "Binary file, diff omitted." }
            } else if no_hunks {
                div { class: "binary-note", "No textual changes." }
            } else {
                for h in hunks {
                    {if side_by_side { render_hunk_sbs(h) } else { render_hunk(h) }}
                }
            }
        }
    }
}

fn render_hunk(h: DiffHunk) -> Element {
    let header = format!(
        "@@ -{},{} +{},{} @@",
        h.old_start, h.old_count, h.new_start, h.new_count
    );
    let lines = h.lines.clone();
    rsx! {
        div { class: "hunk",
            div { class: "hunk-header", "{header}" }
            div { class: "hunk-lines",
                for l in lines {
                    {render_diff_line(l)}
                }
            }
        }
    }
}

fn render_hunk_sbs(h: DiffHunk) -> Element {
    let header = format!(
        "@@ -{},{} +{},{} @@",
        h.old_start, h.old_count, h.new_start, h.new_count
    );
    let rows = pair_sbs_rows(h.lines);
    rsx! {
        div { class: "hunk",
            div { class: "hunk-header", "{header}" }
            div { class: "hunk-sbs",
                for row in rows {
                    {render_sbs_row(row)}
                }
            }
        }
    }
}

#[derive(Clone)]
enum SbsRow {
    Ctx(DiffLine),
    Pair(DiffLine, DiffLine),
    OnlyDel(DiffLine),
    OnlyAdd(DiffLine),
}

fn pair_sbs_rows(lines: Vec<DiffLine>) -> Vec<SbsRow> {
    let mut out = Vec::with_capacity(lines.len());
    let mut dels: Vec<DiffLine> = Vec::new();
    let mut adds: Vec<DiffLine> = Vec::new();

    let flush = |out: &mut Vec<SbsRow>, dels: &mut Vec<DiffLine>, adds: &mut Vec<DiffLine>| {
        let dv = std::mem::take(dels);
        let av = std::mem::take(adds);
        let pairs = dv.len().min(av.len());
        let mut di = dv.into_iter();
        let mut ai = av.into_iter();
        for _ in 0..pairs {
            out.push(SbsRow::Pair(di.next().unwrap(), ai.next().unwrap()));
        }
        for d in di {
            out.push(SbsRow::OnlyDel(d));
        }
        for a in ai {
            out.push(SbsRow::OnlyAdd(a));
        }
    };

    for line in lines {
        match line.kind.as_str() {
            "ctx" => {
                flush(&mut out, &mut dels, &mut adds);
                out.push(SbsRow::Ctx(line));
            }
            "del" => {
                if !adds.is_empty() {
                    flush(&mut out, &mut dels, &mut adds);
                }
                dels.push(line);
            }
            "add" => adds.push(line),
            _ => {}
        }
    }
    flush(&mut out, &mut dels, &mut adds);
    out
}

fn render_sbs_row(row: SbsRow) -> Element {
    match row {
        SbsRow::Ctx(l) => {
            let old_n = l.old_line.map(|n| n.to_string()).unwrap_or_default();
            let new_n = l.new_line.map(|n| n.to_string()).unwrap_or_default();
            let tokens = l.tokens.clone();
            let plain = l.text.clone();
            rsx! {
                div { class: "sbs-row sbs-ctx",
                    span { class: "ln", "{old_n}" }
                    span { class: "txt", {render_line_content(&tokens, &plain)} }
                    span { class: "ln", "{new_n}" }
                    span { class: "txt", {render_line_content(&tokens, &plain)} }
                }
            }
        }
        SbsRow::Pair(d, a) => {
            let old_n = d.old_line.map(|n| n.to_string()).unwrap_or_default();
            let new_n = a.new_line.map(|n| n.to_string()).unwrap_or_default();
            let d_tokens = d.tokens.clone();
            let a_tokens = a.tokens.clone();
            let d_plain = d.text.clone();
            let a_plain = a.text.clone();
            rsx! {
                div { class: "sbs-row sbs-mod",
                    span { class: "ln ln-del", "{old_n}" }
                    span { class: "txt txt-del", {render_line_content(&d_tokens, &d_plain)} }
                    span { class: "ln ln-add", "{new_n}" }
                    span { class: "txt txt-add", {render_line_content(&a_tokens, &a_plain)} }
                }
            }
        }
        SbsRow::OnlyDel(d) => {
            let old_n = d.old_line.map(|n| n.to_string()).unwrap_or_default();
            let tokens = d.tokens.clone();
            let plain = d.text.clone();
            rsx! {
                div { class: "sbs-row sbs-del",
                    span { class: "ln ln-del", "{old_n}" }
                    span { class: "txt txt-del", {render_line_content(&tokens, &plain)} }
                    span { class: "ln empty", "" }
                    span { class: "txt empty", "" }
                }
            }
        }
        SbsRow::OnlyAdd(a) => {
            let new_n = a.new_line.map(|n| n.to_string()).unwrap_or_default();
            let tokens = a.tokens.clone();
            let plain = a.text.clone();
            rsx! {
                div { class: "sbs-row sbs-add",
                    span { class: "ln empty", "" }
                    span { class: "txt empty", "" }
                    span { class: "ln ln-add", "{new_n}" }
                    span { class: "txt txt-add", {render_line_content(&tokens, &plain)} }
                }
            }
        }
    }
}

pub(crate) fn render_line_content(tokens: &Option<Vec<Token>>, plain: &str) -> Element {
    match tokens {
        Some(toks) if !toks.is_empty() => {
            let toks = toks.clone();
            rsx! {
                for t in toks {
                    span { class: "tok tok-{token_class_to_css(&t.class)}", "{t.text}" }
                }
            }
        }
        _ => {
            let plain_owned = plain.to_string();
            rsx! { "{plain_owned}" }
        }
    }
}

fn render_diff_line(l: DiffLine) -> Element {
    let kind = l.kind.clone();
    let old = l.old_line.map(|n| n.to_string()).unwrap_or_default();
    let new = l.new_line.map(|n| n.to_string()).unwrap_or_default();
    let marker = match kind.as_str() {
        "add" => "+",
        "del" => "-",
        _ => " ",
    };
    let tokens = l.tokens.clone();
    let plain_text = l.text.clone();
    rsx! {
        div { class: "diff-line line-{kind}",
            span { class: "ln old", "{old}" }
            span { class: "ln new", "{new}" }
            span { class: "marker", "{marker}" }
            span { class: "text",
                if let Some(toks) = tokens {
                    if toks.is_empty() {
                        {rsx! { "{plain_text}" }}
                    } else {
                        for t in toks {
                            span { class: "tok tok-{token_class_to_css(&t.class)}", "{t.text}" }
                        }
                    }
                } else {
                    {rsx! { "{plain_text}" }}
                }
            }
        }
    }
}

fn token_class_to_css(class: &str) -> String {
    if class.is_empty() {
        "plain".to_string()
    } else {
        class.replace('.', "-")
    }
}
