//! Lane assignment for the commit-graph column rendered next to the
//! log table. The algorithm walks the commit list (HEAD-first, same
//! order the log endpoint returns) and produces one `RowLayout` per
//! commit, recording which lane the commit sits on, which lanes are
//! "alive" coming in / going out, and which other lanes the node
//! connects to (branch and merge edges).
//!
//! Output is pure data — `render_graph_cell` in `main_panel` turns it
//! into SVG. Tests below stay focused on the algorithm.

use dioxus::prelude::*;
use gitrust_types::CommitInfo;

const LANE_W: u32 = 14;
const ROW_H: u32 = 28;
const NODE_R: u32 = 4;

/// Cycled palette. Picked to read well on both light and dark themes.
const COLORS: &[&str] = &[
    "#2563eb", // blue   — lane 0 (HEAD by convention)
    "#dc2626", // red
    "#16a34a", // green
    "#ca8a04", // amber
    "#9333ea", // purple
    "#0891b2", // cyan
    "#ea580c", // orange
    "#64748b", // slate
];

fn lane_color(i: usize) -> &'static str {
    COLORS[i % COLORS.len()]
}

fn lane_x(i: usize) -> u32 {
    i as u32 * LANE_W + LANE_W / 2
}

/// Render one row of the commit graph as an SVG element. `width` is the
/// number of lanes reserved across the whole table — every row uses the
/// same SVG width so nodes line up vertically.
pub fn render_graph_cell(row: &RowLayout, width: usize) -> Element {
    let w_px = (width as u32) * LANE_W;
    let h_px = ROW_H as i32;
    let mid = h_px / 2;
    let node_cx = lane_x(row.node_lane) as i32;

    // Vertical halves are stretched past the SVG viewBox by `OVERHANG`
    // on each side; combined with `overflow: visible`, this bridges the
    // 2-4px gap that table-cell padding leaves between adjacent SVGs so
    // the verticals look continuous across row boundaries. The diagonals
    // stay inside the box so they don't visually overshoot the lane they
    // merge into.
    const OVERHANG: i32 = 4;
    let top_y: i32 = -OVERHANG;
    let bot_y: i32 = h_px + OVERHANG;

    let in_lines: Vec<(i32, i32, i32, i32, &str)> = row
        .in_lanes
        .iter()
        .enumerate()
        .filter_map(|(i, &active)| {
            if active {
                let x = lane_x(i) as i32;
                Some((x, top_y, x, mid, lane_color(i)))
            } else {
                None
            }
        })
        .collect();

    let out_lines: Vec<(i32, i32, i32, i32, &str)> = row
        .out_lanes
        .iter()
        .enumerate()
        .filter_map(|(i, &active)| {
            let in_active = row.in_lanes.get(i).copied().unwrap_or(false);
            let is_node = i == row.node_lane;
            if active && (in_active || is_node) {
                let x = lane_x(i) as i32;
                Some((x, mid, x, bot_y, lane_color(i)))
            } else {
                None
            }
        })
        .collect();

    // Branch and merge diagonals from the node down to other lanes.
    // Endpoint stays at the row bottom so the diagonal lands on the
    // lane without overshooting into the next row.
    let edge_lines: Vec<(i32, i32, i32, i32, &str)> = row
        .edges
        .iter()
        .map(|&t| {
            let x_to = lane_x(t) as i32;
            (node_cx, mid, x_to, h_px, lane_color(t))
        })
        .collect();

    rsx! {
        svg {
            class: "graph-svg",
            width: "{w_px}",
            height: "{h_px}",
            view_box: "0 0 {w_px} {h_px}",
            shape_rendering: "geometricPrecision",
            overflow: "visible",
            for (x1, y1, x2, y2, col) in in_lines {
                line {
                    x1: "{x1}", y1: "{y1}", x2: "{x2}", y2: "{y2}",
                    style: "stroke: {col}; stroke-width: 1.6; stroke-linecap: butt;",
                }
            }
            for (x1, y1, x2, y2, col) in out_lines {
                line {
                    x1: "{x1}", y1: "{y1}", x2: "{x2}", y2: "{y2}",
                    style: "stroke: {col}; stroke-width: 1.6; stroke-linecap: butt;",
                }
            }
            for (x1, y1, x2, y2, col) in edge_lines {
                line {
                    x1: "{x1}", y1: "{y1}", x2: "{x2}", y2: "{y2}",
                    style: "stroke: {col}; stroke-width: 1.6; stroke-linecap: round;",
                }
            }
            circle {
                cx: "{node_cx}",
                cy: "{mid}",
                r: "{NODE_R}",
                fill: "{lane_color(row.node_lane)}",
                stroke: "var(--surface)",
                style: "stroke-width: 1.5;",
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RowLayout {
    /// Column the commit's node sits on (0-indexed).
    pub node_lane: usize,
    /// Active lanes on entry — vertical line drawn in each `true` column
    /// from the row's top to its mid-line. The lane at `node_lane`, when
    /// active here, terminates at the node.
    pub in_lanes: Vec<bool>,
    /// Active lanes on exit — vertical line drawn in each `true` column
    /// from the row's mid-line to its bottom. Continuations through this
    /// row plus newly opened branches both show up here.
    pub out_lanes: Vec<bool>,
    /// Lanes the node draws a diagonal down to (other than `node_lane`).
    /// Covers both branches (newly assigned lane) and merges (lane was
    /// already pending another parent).
    pub edges: Vec<usize>,
}

pub fn compute_graph(commits: &[CommitInfo]) -> Vec<RowLayout> {
    let mut lanes: Vec<Option<String>> = Vec::new();
    let mut rows = Vec::with_capacity(commits.len());
    for c in commits {
        let in_lanes: Vec<bool> = lanes.iter().map(Option::is_some).collect();

        // Locate the lane that was waiting for this commit, or allocate one.
        let node_lane = match lanes
            .iter()
            .position(|s| s.as_deref() == Some(c.oid.as_str()))
        {
            Some(i) => i,
            None => first_free_or_push(&mut lanes),
        };
        lanes[node_lane] = None;

        let mut edges: Vec<usize> = Vec::new();
        for (i, parent) in c.parents.iter().enumerate() {
            // Parent already pending elsewhere → this is a merge edge.
            if let Some(j) = lanes
                .iter()
                .position(|s| s.as_deref() == Some(parent.as_str()))
            {
                if j != node_lane && !edges.contains(&j) {
                    edges.push(j);
                }
                continue;
            }
            // First parent prefers continuation on the node's lane so a
            // linear history stays in one column.
            if i == 0 && lanes[node_lane].is_none() {
                lanes[node_lane] = Some(parent.clone());
                continue;
            }
            // Otherwise allocate a new lane.
            let new_lane = first_free_or_push(&mut lanes);
            lanes[new_lane] = Some(parent.clone());
            if new_lane != node_lane && !edges.contains(&new_lane) {
                edges.push(new_lane);
            }
        }

        let out_lanes: Vec<bool> = lanes.iter().map(Option::is_some).collect();
        rows.push(RowLayout {
            node_lane,
            in_lanes,
            out_lanes,
            edges,
        });
    }
    rows
}

fn first_free_or_push(lanes: &mut Vec<Option<String>>) -> usize {
    if let Some(i) = lanes.iter().position(Option::is_none) {
        i
    } else {
        lanes.push(None);
        lanes.len() - 1
    }
}

/// Max lane index used across all rows, +1. Drives the SVG width so
/// every row's coordinate system lines up.
pub fn graph_width(rows: &[RowLayout]) -> usize {
    rows.iter()
        .map(|r| {
            let from_in = r.in_lanes.iter().rposition(|&b| b).map_or(0, |i| i + 1);
            let from_out = r.out_lanes.iter().rposition(|&b| b).map_or(0, |i| i + 1);
            let from_edge = r.edges.iter().copied().max().map(|i| i + 1).unwrap_or(0);
            from_in.max(from_out).max(r.node_lane + 1).max(from_edge)
        })
        .max()
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ci(oid: &str, parents: &[&str]) -> CommitInfo {
        CommitInfo {
            oid: oid.into(),
            short_oid: oid.chars().take(8).collect(),
            summary: "".into(),
            body: "".into(),
            parents: parents.iter().map(|s| (*s).into()).collect(),
            author_name: "".into(),
            author_email: "".into(),
            time_unix: 0,
        }
    }

    #[test]
    fn empty_input_yields_empty_output() {
        assert!(compute_graph(&[]).is_empty());
    }

    #[test]
    fn linear_history_stays_on_lane_zero() {
        let commits = [ci("a", &["b"]), ci("b", &["c"]), ci("c", &[])];
        let g = compute_graph(&commits);
        assert_eq!(g.len(), 3);
        for r in &g {
            assert_eq!(r.node_lane, 0);
            assert!(r.edges.is_empty());
        }
        // Tip has nothing coming in, root has nothing going out.
        assert_eq!(g[0].in_lanes, vec![] as Vec<bool>);
        assert_eq!(g[0].out_lanes, vec![true]);
        assert_eq!(g[2].in_lanes, vec![true]);
        assert_eq!(g[2].out_lanes, vec![false]);
    }

    #[test]
    fn merge_with_two_parents_opens_a_second_lane() {
        // M ──┬─ B ── D
        //     └─ C ── D
        let commits = [
            ci("m", &["b", "c"]),
            ci("b", &["d"]),
            ci("c", &["d"]),
            ci("d", &[]),
        ];
        let g = compute_graph(&commits);

        // M: branch edge from lane 0 to lane 1.
        assert_eq!(g[0].node_lane, 0);
        assert_eq!(g[0].edges, vec![1]);
        assert_eq!(g[0].out_lanes, vec![true, true]);

        // B continues on lane 0; C runs alongside on lane 1.
        assert_eq!(g[1].node_lane, 0);
        assert_eq!(g[1].in_lanes, vec![true, true]);
        assert!(g[1].edges.is_empty());

        // C merges back: node on lane 1, merge edge to lane 0 (D already pending).
        assert_eq!(g[2].node_lane, 1);
        assert_eq!(g[2].edges, vec![0]);
        // Lane 1 closes after C; lane 0 keeps going.
        assert_eq!(g[2].out_lanes, vec![true, false]);

        // D consumes lane 0 and terminates the graph.
        assert_eq!(g[3].node_lane, 0);
        assert_eq!(g[3].out_lanes, vec![false, false]);
    }

    #[test]
    fn octopus_merge_opens_extra_lanes() {
        let commits = [
            ci("o", &["b", "c", "d"]),
            ci("b", &["x"]),
            ci("c", &["x"]),
            ci("d", &["x"]),
            ci("x", &[]),
        ];
        let g = compute_graph(&commits);
        assert_eq!(g[0].node_lane, 0);
        // Two extra edges from the merge node — one per non-primary parent.
        assert_eq!(g[0].edges, vec![1, 2]);
        // The merges back to the single ancestor all land on lane 0.
        assert_eq!(g[1].node_lane, 0);
        assert!(g[1].edges.is_empty());
        assert_eq!(g[2].node_lane, 1);
        assert_eq!(g[2].edges, vec![0]);
        assert_eq!(g[3].node_lane, 2);
        assert_eq!(g[3].edges, vec![0]);
        assert_eq!(g[4].node_lane, 0);
    }

    #[test]
    fn already_pending_parent_does_not_consume_a_new_lane() {
        // a, b both descend from c. a is HEAD; b reappears as a's parent's
        // sibling. Verifies that when a parent already sits in another
        // lane, we emit a merge edge instead of a second allocation.
        let commits = [
            ci("a", &["c", "b"]), // merge-style: two parents
            ci("b", &["c"]),
            ci("c", &[]),
        ];
        let g = compute_graph(&commits);
        // a places c on lane 0 (first parent) and b on lane 1.
        assert_eq!(g[0].node_lane, 0);
        assert_eq!(g[0].edges, vec![1]);
        // b's parent c is already pending on lane 0 → merge edge, not branch.
        assert_eq!(g[1].node_lane, 1);
        assert_eq!(g[1].edges, vec![0]);
        // c finishes both.
        assert_eq!(g[2].node_lane, 0);
        assert_eq!(g[2].out_lanes, vec![false, false]);
    }

    #[test]
    fn truncated_log_leaves_lanes_dangling_off_bottom() {
        // Only the top two commits of a chain — c is never popped.
        let commits = [ci("a", &["b"]), ci("b", &["c"])];
        let g = compute_graph(&commits);
        assert_eq!(g[1].out_lanes, vec![true]);
    }

    #[test]
    fn graph_width_reports_max_lane_in_use() {
        let commits = [
            ci("o", &["b", "c", "d"]),
            ci("b", &["x"]),
            ci("c", &["x"]),
            ci("d", &["x"]),
            ci("x", &[]),
        ];
        let g = compute_graph(&commits);
        assert_eq!(graph_width(&g), 3);
    }
}
