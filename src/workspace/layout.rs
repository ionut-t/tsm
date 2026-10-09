//! tmux `#{window_layout}` strings: parsing them into a tree of cells,
//! rendering a tree back into a string, and converting a tree into the nested
//! rows and panes that workspace TOML uses.
//!
//! A layout string looks like `f26b,120x40,0,0[120x27,0,0,0,120x12,0,28{...}]`:
//! a checksum, then a tree of cells. Each cell is `WxH,X,Y` followed by either
//! `,<pane id>` (a leaf), `{...}` (children side by side) or `[...]` (children
//! stacked top to bottom).

use std::fmt::Write as _;

use crate::error::{Result, TsmError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutCell {
    pub width: u32,
    pub height: u32,
    pub kind: CellKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CellKind {
    /// A leaf pane. The number is the pane id without its `%` prefix.
    Pane(u32),
    /// Children laid out side by side (`{...}`).
    LeftRight(Vec<LayoutCell>),
    /// Children stacked top to bottom (`[...]`).
    TopBottom(Vec<LayoutCell>),
}

/// One `[[...row]]` worth of panes.
///
/// Row heights are deliberately not captured: saved workspaces leave row
/// heights to tmux.
#[derive(Debug, PartialEq, Eq)]
pub struct RowShape {
    pub panes: Vec<PaneShape>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct PaneShape {
    /// Percentage of the containing row's width. `None` for the last pane in
    /// a row, and for every pane when the row is split evenly.
    pub width: Option<u32>,
    pub content: PaneContent,
}

#[derive(Debug, PartialEq, Eq)]
pub enum PaneContent {
    /// A real pane, by tmux pane id (e.g. `%3`), for looking up its command
    /// and path.
    Leaf(String),
    /// A pane split further into rows.
    Rows(Vec<RowShape>),
}

pub fn parse(layout: &str) -> Result<LayoutCell> {
    let (_checksum, body) = layout
        .split_once(',')
        .ok_or_else(|| parse_error(layout, "missing checksum"))?;

    let mut parser = Parser {
        src: layout,
        s: body,
        pos: 0,
    };
    let cell = parser.cell()?;
    if parser.pos != body.len() {
        return Err(parser.error("unexpected trailing input"));
    }
    Ok(cell)
}

/// Render a tree as a layout string `select-layout` accepts. Children are
/// placed one after another with a one-cell border between them, and pane
/// ids are kept as given (tmux fills the layout with the window's panes in
/// order, whatever their ids).
pub fn render(cell: &LayoutCell) -> String {
    let mut body = String::new();
    write_cell(&mut body, cell, 0, 0);
    format!("{:04x},{body}", checksum(&body))
}

fn write_cell(out: &mut String, cell: &LayoutCell, x: u32, y: u32) {
    let _ = write!(out, "{}x{},{x},{y}", cell.width, cell.height);
    match &cell.kind {
        CellKind::Pane(id) => {
            let _ = write!(out, ",{id}");
        }
        CellKind::LeftRight(children) => {
            out.push('{');
            let mut cx = x;
            for (i, child) in children.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_cell(out, child, cx, y);
                cx += child.width + 1;
            }
            out.push('}');
        }
        CellKind::TopBottom(children) => {
            out.push('[');
            let mut cy = y;
            for (i, child) in children.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_cell(out, child, x, cy);
                cy += child.height + 1;
            }
            out.push(']');
        }
    }
}

/// tmux's layout checksum (`layout_checksum` in tmux's source).
fn checksum(body: &str) -> u16 {
    body.bytes().fold(0u16, |csum, byte| {
        (csum >> 1)
            .wrapping_add((csum & 1) << 15)
            .wrapping_add(u16::from(byte))
    })
}

/// Convert a layout into rows of panes, where a pane may hold rows of its
/// own. Any layout tmux can make fits this shape.
pub fn to_rows(cell: &LayoutCell) -> Vec<RowShape> {
    match &cell.kind {
        CellKind::TopBottom(children) => children.iter().map(to_row).collect(),
        _ => vec![to_row(cell)],
    }
}

fn to_row(cell: &LayoutCell) -> RowShape {
    match &cell.kind {
        CellKind::LeftRight(children) => {
            let widths = widths(children, cell.width);
            RowShape {
                panes: children
                    .iter()
                    .zip(widths)
                    .map(|(child, width)| to_pane(child, width))
                    .collect(),
            }
        }
        _ => RowShape {
            panes: vec![to_pane(cell, None)],
        },
    }
}

fn to_pane(cell: &LayoutCell, width: Option<u32>) -> PaneShape {
    let content = match &cell.kind {
        CellKind::Pane(id) => PaneContent::Leaf(format!("%{id}")),
        _ => PaneContent::Rows(to_rows(cell)),
    };
    PaneShape { width, content }
}

/// Widths for side-by-side panes as percentages of the row: none at all when
/// the row is split evenly (launch splits evenly by default), otherwise one
/// for every pane but the last, which takes what's left.
fn widths(children: &[LayoutCell], row_width: u32) -> Vec<Option<u32>> {
    let min = children.iter().map(|c| c.width).min().unwrap_or(0);
    let max = children.iter().map(|c| c.width).max().unwrap_or(0);
    let last = children.len() - 1;

    children
        .iter()
        .enumerate()
        .map(|(i, child)| (max - min > 1 && i != last).then(|| percent(child.width, row_width)))
        .collect()
}

/// `part` as a rounded percentage of `whole`, kept within 1..=99.
fn percent(part: u32, whole: u32) -> u32 {
    let pct = (part * 200 + whole) / (whole * 2);
    pct.clamp(1, 99)
}

fn parse_error(layout: &str, reason: &str) -> TsmError {
    TsmError::LayoutParse(format!("{reason} in '{layout}'"))
}

struct Parser<'a> {
    /// The full layout string, for error messages.
    src: &'a str,
    /// The layout without its checksum.
    s: &'a str,
    pos: usize,
}

impl Parser<'_> {
    fn cell(&mut self) -> Result<LayoutCell> {
        let width = self.number()?;
        self.expect(b'x')?;
        let height = self.number()?;
        self.expect(b',')?;
        self.number()?; // x offset
        self.expect(b',')?;
        self.number()?; // y offset

        let kind = match self.peek() {
            Some(b',') => {
                self.pos += 1;
                CellKind::Pane(self.number()?)
            }
            Some(b'{') => CellKind::LeftRight(self.children(b'}')?),
            Some(b'[') => CellKind::TopBottom(self.children(b']')?),
            _ => return Err(self.error("expected ',', '{' or '['")),
        };

        Ok(LayoutCell {
            width,
            height,
            kind,
        })
    }

    fn children(&mut self, close: u8) -> Result<Vec<LayoutCell>> {
        self.pos += 1; // opening bracket
        let mut cells = vec![self.cell()?];
        loop {
            match self.peek() {
                Some(b',') => {
                    self.pos += 1;
                    cells.push(self.cell()?);
                }
                Some(c) if c == close => {
                    self.pos += 1;
                    return Ok(cells);
                }
                _ => return Err(self.error(&format!("expected ',' or '{}'", close as char))),
            }
        }
    }

    fn number(&mut self) -> Result<u32> {
        let start = self.pos;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.pos += 1;
        }
        self.s[start..self.pos]
            .parse()
            .map_err(|_| self.error("expected a number"))
    }

    fn expect(&mut self, byte: u8) -> Result<()> {
        if self.peek() == Some(byte) {
            self.pos += 1;
            Ok(())
        } else {
            Err(self.error(&format!("expected '{}'", byte as char)))
        }
    }

    fn peek(&self) -> Option<u8> {
        self.s.as_bytes().get(self.pos).copied()
    }

    fn error(&self, reason: &str) -> TsmError {
        parse_error(self.src, &format!("{reason} at offset {}", self.pos))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Captured from tmux 3.7 on a 120x40 window.
    const SINGLE: &str = "55af,120x40,0,0,10";
    const ROWS: &str = "f26b,120x40,0,0[120x27,0,0,0,120x12,0,28{60x12,0,28,1,59x12,61,28,2}]";
    const THREE_COLUMNS: &str = "b807,120x40,0,0{30x40,0,0,0,29x40,31,0,2,59x40,61,0,1}";
    const COLUMNS_FIRST: &str =
        "58e8,120x40,0,0{60x40,0,0,3,59x40,61,0[59x20,61,0,4,59x19,61,21,5]}";
    const NESTED: &str = "5c54,120x40,0,0[120x20,0,0,6,120x19,0,21{60x19,0,21,7,59x19,61,21[59x9,61,21,8,59x9,61,31,9]}]";
    // From a real session on a 152x39 terminal.
    const BIG_LEFT: &str = "bcd7,152x39,0,0{76x39,0,0,88,75x39,77,0[75x19,77,0,89,75x19,77,20{37x19,77,20,90,37x19,115,20,91}]}";

    fn pane(width: u32, height: u32, id: u32) -> LayoutCell {
        LayoutCell {
            width,
            height,
            kind: CellKind::Pane(id),
        }
    }

    fn leaf(id: &str, width: Option<u32>) -> PaneShape {
        PaneShape {
            width,
            content: PaneContent::Leaf(id.to_string()),
        }
    }

    fn split(width: Option<u32>, rows: Vec<RowShape>) -> PaneShape {
        PaneShape {
            width,
            content: PaneContent::Rows(rows),
        }
    }

    fn row(panes: Vec<PaneShape>) -> RowShape {
        RowShape { panes }
    }

    #[test]
    fn parses_single_pane() {
        assert_eq!(parse(SINGLE).unwrap(), pane(120, 40, 10));
    }

    #[test]
    fn parses_nested_splits() {
        let expected = LayoutCell {
            width: 120,
            height: 40,
            kind: CellKind::TopBottom(vec![
                pane(120, 27, 0),
                LayoutCell {
                    width: 120,
                    height: 12,
                    kind: CellKind::LeftRight(vec![pane(60, 12, 1), pane(59, 12, 2)]),
                },
            ]),
        };
        assert_eq!(parse(ROWS).unwrap(), expected);
    }

    #[test]
    fn parses_three_levels_deep() {
        let root = parse(NESTED).unwrap();
        let CellKind::TopBottom(rows) = &root.kind else {
            panic!("expected rows, got {root:?}");
        };
        let CellKind::LeftRight(cols) = &rows[1].kind else {
            panic!("expected columns, got {:?}", rows[1]);
        };
        assert!(matches!(&cols[1].kind, CellKind::TopBottom(c) if c.len() == 2));
    }

    #[test]
    fn rejects_malformed_layouts() {
        for bad in [
            "",
            "f26b",
            "f26b,",
            "f26b,120x40",
            "f26b,120x40,0,0",
            "f26b,120x40,0,0,",
            "f26b,120x40,0,0[120x27,0,0,0",
            "f26b,120x40,0,0[120x27,0,0,0}",
            "f26b,120x40,0,0,1junk",
            "f26b,ax40,0,0,1",
        ] {
            let err = parse(bad).expect_err(bad);
            assert!(matches!(err, TsmError::LayoutParse(_)), "{bad}: {err}");
        }
    }

    #[test]
    fn render_reproduces_tmux_layouts_exactly() {
        for layout in [SINGLE, ROWS, THREE_COLUMNS, COLUMNS_FIRST, NESTED, BIG_LEFT] {
            assert_eq!(render(&parse(layout).unwrap()), layout);
        }
    }

    #[test]
    fn checksum_matches_tmux() {
        assert_eq!(checksum("120x40,0,0,10"), 0x55af);
        assert_eq!(checksum(BIG_LEFT.split_once(',').unwrap().1), 0xbcd7);
    }

    #[test]
    fn single_pane_becomes_one_row_without_sizes() {
        let rows = to_rows(&parse(SINGLE).unwrap());
        assert_eq!(rows, vec![row(vec![leaf("%10", None)])]);
    }

    #[test]
    fn even_splits_get_no_widths() {
        let rows = to_rows(&parse(ROWS).unwrap());
        assert_eq!(
            rows,
            vec![
                row(vec![leaf("%0", None)]),
                row(vec![leaf("%1", None), leaf("%2", None)]),
            ]
        );
    }

    #[test]
    fn uneven_splits_get_widths_except_the_last_pane() {
        let rows = to_rows(&parse(THREE_COLUMNS).unwrap());
        assert_eq!(
            rows,
            vec![row(vec![
                leaf("%0", Some(25)),
                leaf("%2", Some(24)),
                leaf("%1", None),
            ])]
        );
    }

    #[test]
    fn columns_first_become_a_pane_with_rows() {
        let rows = to_rows(&parse(COLUMNS_FIRST).unwrap());
        assert_eq!(
            rows,
            vec![row(vec![
                leaf("%3", None),
                split(
                    None,
                    vec![row(vec![leaf("%4", None)]), row(vec![leaf("%5", None)])]
                ),
            ])]
        );
    }

    #[test]
    fn deep_nesting_alternates_rows_and_panes() {
        let rows = to_rows(&parse(BIG_LEFT).unwrap());
        assert_eq!(
            rows,
            vec![row(vec![
                leaf("%88", None),
                split(
                    None,
                    vec![
                        row(vec![leaf("%89", None)]),
                        row(vec![leaf("%90", None), leaf("%91", None)]),
                    ]
                ),
            ])]
        );
    }

    #[test]
    fn widths_are_relative_to_the_containing_row() {
        // The right half (60 wide) holds 45/14 columns: 75% of its row.
        let layout = "0000,120x40,0,0{59x40,0,0,0,60x40,60,0[60x20,60,0,1,60x19,60,21{45x19,60,21,2,14x19,106,21,3}]}";
        let rows = to_rows(&parse(layout).unwrap());
        let PaneContent::Rows(inner) = &rows[0].panes[1].content else {
            panic!("expected a split pane");
        };
        assert_eq!(inner[1].panes[0].width, Some(75));
        assert_eq!(inner[1].panes[1].width, None);
    }

    #[test]
    fn percent_rounds_and_stays_resizable() {
        assert_eq!(percent(60, 120), 50);
        assert_eq!(percent(27, 40), 68);
        assert_eq!(percent(0, 40), 1);
        assert_eq!(percent(40, 40), 99);
    }
}
