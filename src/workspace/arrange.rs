//! Turn a window's rows and panes into a tmux layout.
//!
//! Rows stack top to bottom, panes sit side by side within a row, and a pane
//! can hold rows of its own, so any tmux layout can be described. Launching
//! creates one tmux pane per leaf pane, then applies the layout built here
//! for the window's actual size.

use std::collections::HashMap;

use crate::error::{Result, TsmError};
use crate::workspace::config::{Pane, Row, Window};
use crate::workspace::layout::{CellKind, LayoutCell};

/// A pane tmux will actually create, in the order tmux fills a layout: left
/// to right, top to bottom, depth first.
pub struct Leaf<'a> {
    /// `None` for the implicit pane of an empty window or row.
    pub pane: Option<&'a Pane>,
    /// Effective env: workspace, window, then every enclosing pane's env,
    /// innermost last.
    pub env: HashMap<String, String>,
}

/// Check that panes holding rows don't also set things only a real pane can
/// have.
pub fn validate(window: &Window) -> Result<()> {
    validate_rows(window, &window.row)
}

fn validate_rows(window: &Window, rows: &[Row]) -> Result<()> {
    for pane in rows.iter().flat_map(|r| &r.pane) {
        if pane.row.is_empty() {
            continue;
        }
        if pane.command.is_some() || pane.focus {
            let label = match &window.name {
                Some(name) => format!("window '{name}'"),
                None => "a window".to_string(),
            };
            return Err(TsmError::InvalidArgument(format!(
                "{label}: a pane with rows can't have a command or focus; set them on the panes inside it"
            )));
        }
        validate_rows(window, &pane.row)?;
    }
    Ok(())
}

pub fn leaves<'a>(window: &'a Window, workspace_env: &HashMap<String, String>) -> Vec<Leaf<'a>> {
    let mut out = Vec::new();
    collect_rows(
        &window.row,
        &merge_env(workspace_env, &window.env),
        &mut out,
    );
    out
}

fn collect_rows<'a>(rows: &'a [Row], env: &HashMap<String, String>, out: &mut Vec<Leaf<'a>>) {
    if rows.is_empty() {
        out.push(Leaf {
            pane: None,
            env: env.clone(),
        });
    }
    for row in rows {
        if row.pane.is_empty() {
            out.push(Leaf {
                pane: None,
                env: env.clone(),
            });
        }
        for pane in &row.pane {
            let env = merge_env(env, &pane.env);
            if pane.row.is_empty() {
                out.push(Leaf {
                    pane: Some(pane),
                    env,
                });
            } else {
                collect_rows(&pane.row, &env, out);
            }
        }
    }
}

/// The window's layout at `width` x `height`. Leaves are numbered from 0 in
/// [`leaves`] order; tmux ignores the numbers and fills the layout with the
/// window's panes in order.
pub fn layout(window: &Window, width: u32, height: u32) -> Result<LayoutCell> {
    let mut next_id = 0;
    size(&rows_node(&window.row), width, height, &mut next_id).ok_or_else(|| {
        TsmError::InvalidArgument(format!(
            "the window ({width}x{height}) is too small for all of its panes"
        ))
    })
}

enum Node {
    Leaf,
    Split {
        left_right: bool,
        /// Each child with its requested size, as a percentage of this split.
        children: Vec<(Option<u32>, Node)>,
    },
}

fn rows_node(rows: &[Row]) -> Node {
    match rows {
        [] => Node::Leaf,
        [row] => row_node(row),
        _ => Node::Split {
            left_right: false,
            children: rows.iter().map(|r| (r.height, row_node(r))).collect(),
        },
    }
}

fn row_node(row: &Row) -> Node {
    match &row.pane[..] {
        [] => Node::Leaf,
        [pane] => pane_node(pane),
        panes => Node::Split {
            left_right: true,
            children: panes.iter().map(|p| (p.width, pane_node(p))).collect(),
        },
    }
}

fn pane_node(pane: &Pane) -> Node {
    if pane.row.is_empty() {
        Node::Leaf
    } else {
        rows_node(&pane.row)
    }
}

fn size(node: &Node, width: u32, height: u32, next_id: &mut u32) -> Option<LayoutCell> {
    let kind = match node {
        Node::Leaf => {
            *next_id += 1;
            CellKind::Pane(*next_id - 1)
        }
        Node::Split {
            left_right,
            children,
        } => {
            let total = if *left_right { width } else { height };
            let requested: Vec<_> = children.iter().map(|(pct, _)| *pct).collect();
            let sizes = apportion(total, &requested)?;
            let cells = children
                .iter()
                .zip(sizes)
                .map(|((_, child), s)| {
                    if *left_right {
                        size(child, s, height, next_id)
                    } else {
                        size(child, width, s, next_id)
                    }
                })
                .collect::<Option<Vec<_>>>()?;
            if *left_right {
                CellKind::LeftRight(cells)
            } else {
                CellKind::TopBottom(cells)
            }
        }
    };
    Some(LayoutCell {
        width,
        height,
        kind,
    })
}

/// Split `total` cells between children separated by one-cell borders.
///
/// A child with a percentage gets that share of `total`; the others split
/// what's left evenly. If the requests don't add up, everything is scaled to
/// fit. Leftover cells go to the earliest children, as tmux does. Returns
/// `None` when there isn't room for every child to get at least one cell.
fn apportion(total: u32, requested: &[Option<u32>]) -> Option<Vec<u32>> {
    let n = requested.len() as u32;
    let avail = total.checked_sub(n - 1).filter(|&a| a >= n)?;

    let sized: f64 = requested
        .iter()
        .flatten()
        .map(|&pct| f64::from(pct.min(100)) * f64::from(total) / 100.0)
        .sum();
    let unsized_count = requested.iter().filter(|r| r.is_none()).count() as f64;
    let unsized_share = if unsized_count > 0.0 {
        (f64::from(avail) - sized).max(0.0) / unsized_count
    } else {
        0.0
    };

    let targets: Vec<f64> = requested
        .iter()
        .map(|r| match r {
            Some(pct) => f64::from((*pct).min(100)) * f64::from(total) / 100.0,
            None => unsized_share,
        })
        .collect();
    let sum: f64 = targets.iter().sum();
    let scaled: Vec<f64> = if sum > 0.0 {
        targets.iter().map(|t| t * f64::from(avail) / sum).collect()
    } else {
        vec![f64::from(avail) / f64::from(n); n as usize]
    };

    let mut sizes: Vec<u32> = scaled.iter().map(|s| (s.floor() as u32).max(1)).collect();
    let mut diff = i64::from(avail) - sizes.iter().map(|&s| i64::from(s)).sum::<i64>();

    // Hand out spare cells by largest remainder, earliest first on ties.
    let mut order: Vec<usize> = (0..sizes.len()).collect();
    order.sort_by(|&a, &b| {
        let ra = scaled[a] - scaled[a].floor();
        let rb = scaled[b] - scaled[b].floor();
        rb.total_cmp(&ra).then(a.cmp(&b))
    });
    for &i in order.iter().cycle().take(diff.max(0) as usize) {
        sizes[i] += 1;
    }
    // Take back cells the one-cell minimum overspent, from the largest.
    while diff < 0 {
        let i = (0..sizes.len()).max_by_key(|&i| (sizes[i], std::cmp::Reverse(i)))?;
        if sizes[i] <= 1 {
            return None;
        }
        sizes[i] -= 1;
        diff += 1;
    }

    Some(sizes)
}

fn merge_env(
    base: &HashMap<String, String>,
    overrides: &HashMap<String, String>,
) -> HashMap<String, String> {
    let mut merged = base.clone();
    merged.extend(overrides.iter().map(|(k, v)| (k.clone(), v.clone())));
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::layout::render;

    /// A rendered layout without its checksum (checked in `layout`'s tests).
    fn body(cell: &LayoutCell) -> String {
        render(cell).split_once(',').unwrap().1.to_string()
    }

    fn window(toml: &str) -> Window {
        let mut ws: crate::workspace::config::Workspace =
            toml::from_str(&format!("name = \"t\"\n{toml}")).unwrap();
        ws.window.remove(0)
    }

    fn map(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    // The example from the README: one tall pane on the left, the right half
    // split into a top pane and two bottom panes.
    const BIG_LEFT: &str = r#"
        [[window]]
        [[window.row]]
        [[window.row.pane]]
        command = "1"
        [[window.row.pane]]
        [[window.row.pane.row]]
        [[window.row.pane.row.pane]]
        command = "2"
        [[window.row.pane.row]]
        [[window.row.pane.row.pane]]
        command = "3"
        [[window.row.pane.row.pane]]
        command = "4"
    "#;

    #[test]
    fn nested_rows_build_the_tmux_layout() {
        // tmux produced the same geometry at 152x39 (pane ids differ).
        let w = window(BIG_LEFT);
        assert_eq!(
            body(&layout(&w, 152, 39).unwrap()),
            "152x39,0,0{76x39,0,0,0,75x39,77,0[75x19,77,0,1,75x19,77,20{37x19,77,20,2,37x19,115,20,3}]}"
        );
    }

    #[test]
    fn leaves_follow_layout_order() {
        let w = window(BIG_LEFT);
        let commands: Vec<_> = leaves(&w, &HashMap::new())
            .iter()
            .map(|l| l.pane.and_then(|p| p.command.clone()).unwrap())
            .collect();
        assert_eq!(commands, ["1", "2", "3", "4"]);
    }

    #[test]
    fn empty_windows_and_rows_have_one_implicit_pane() {
        let w = window("[[window]]");
        assert_eq!(leaves(&w, &HashMap::new()).len(), 1);
        assert_eq!(body(&layout(&w, 80, 24).unwrap()), "80x24,0,0,0");

        let w = window("[[window]]\n[[window.row]]\n[[window.row]]");
        let found = leaves(&w, &HashMap::new());
        assert_eq!(found.len(), 2);
        assert!(found.iter().all(|l| l.pane.is_none()));
    }

    #[test]
    fn rows_without_sizes_split_evenly() {
        let w = window("[[window]]\n[[window.row]]\n[[window.row]]\n[[window.row]]");
        let LayoutCell {
            kind: CellKind::TopBottom(rows),
            ..
        } = layout(&w, 120, 40).unwrap()
        else {
            panic!("expected rows");
        };
        let heights: Vec<_> = rows.iter().map(|r| r.height).collect();
        // 38 cells after borders; the spare ones go to the earliest rows.
        assert_eq!(heights, [13, 13, 12]);
    }

    #[test]
    fn widths_are_a_share_of_the_containing_row() {
        let w = window(
            r#"
            [[window]]
            [[window.row]]
            [[window.row.pane]]
            width = 30
            [[window.row.pane]]
            [[window.row.pane.row]]
            [[window.row.pane.row.pane]]
            width = 75
            [[window.row.pane.row.pane]]
            "#,
        );
        let cell = layout(&w, 120, 40).unwrap();
        let CellKind::LeftRight(cols) = &cell.kind else {
            panic!("expected columns");
        };
        assert_eq!(cols[0].width, 36);
        assert_eq!(cols[1].width, 83);
        let CellKind::LeftRight(inner) = &cols[1].kind else {
            panic!("expected nested columns");
        };
        // 75% of the 83-cell right side.
        assert_eq!(inner[0].width, 62);
        assert_eq!(inner[1].width, 20);
    }

    #[test]
    fn apportion_scales_requests_that_do_not_fit() {
        // Two panes asking for 80% each get equal halves.
        assert_eq!(apportion(120, &[Some(80), Some(80)]), Some(vec![60, 59]));
        // A sized pane leaving nothing still gives the rest one cell.
        assert_eq!(apportion(10, &[Some(100), None]), Some(vec![8, 1]));
        assert_eq!(apportion(3, &[None, None, None]), None);
        assert_eq!(apportion(5, &[None, None, None]), Some(vec![1, 1, 1]));
    }

    #[test]
    fn too_small_windows_are_an_error() {
        let w = window(BIG_LEFT);
        assert!(layout(&w, 4, 3).is_err());
    }

    #[test]
    fn env_cascades_through_enclosing_panes() {
        let w = window(
            r#"
            [[window]]
            [window.env]
            W = "w"
            [[window.row]]
            [[window.row.pane]]
            [window.row.pane.env]
            OUTER = "o"
            W = "pane wins"
            [[window.row.pane.row]]
            [[window.row.pane.row.pane]]
            [window.row.pane.row.pane.env]
            INNER = "i"
            "#,
        );
        let found = leaves(&w, &map(&[("WS", "1"), ("W", "ws")]));
        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0].env,
            map(&[
                ("WS", "1"),
                ("W", "pane wins"),
                ("OUTER", "o"),
                ("INNER", "i")
            ])
        );
    }

    #[test]
    fn containers_cannot_run_commands_or_take_focus() {
        for setting in ["command = \"x\"", "focus = true"] {
            let w = window(&format!(
                "[[window]]\nname = \"dev\"\n[[window.row]]\n[[window.row.pane]]\n{setting}\n[[window.row.pane.row]]"
            ));
            let err = validate(&w).unwrap_err();
            assert!(
                err.to_string()
                    .starts_with("window 'dev': a pane with rows"),
                "{err}"
            );
        }
        assert!(validate(&window(BIG_LEFT)).is_ok());
    }

    #[test]
    fn merge_env_overlays_overrides_onto_base() {
        let merged = merge_env(
            &map(&[("A", "1"), ("B", "2")]),
            &map(&[("B", "20"), ("C", "3")]),
        );
        assert_eq!(merged, map(&[("A", "1"), ("B", "20"), ("C", "3")]));
    }
}
