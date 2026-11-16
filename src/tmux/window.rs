#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Window {
    pub session_name: String,
    pub index: u32,
    pub name: String,
    pub pane_id: String,
}

/// The adjacent swaps that move the window at `source` to `target`'s position,
/// shifting every window in between by one. `indices` are the session's window
/// indices in any order. Gaps are respected: "adjacent" means next in sorted
/// order, not ±1.
///
/// Returns `None` if either index is missing, and an empty list if
/// `source == target`.
pub fn reorder_swaps(indices: &[u32], source: u32, target: u32) -> Option<Vec<(u32, u32)>> {
    let mut sorted = indices.to_vec();
    sorted.sort_unstable();

    let from = sorted.iter().position(|&i| i == source)?;
    let to = sorted.iter().position(|&i| i == target)?;

    let swaps = if from < to {
        // Moving right: swap with the right-hand neighbour step by step.
        (from..to).map(|p| (sorted[p], sorted[p + 1])).collect()
    } else {
        // Moving left (or no-op): swap with the left-hand neighbour step by step.
        (to..from)
            .rev()
            .map(|p| (sorted[p + 1], sorted[p]))
            .collect()
    };
    Some(swaps)
}

#[test]
fn moves_left() {
    assert_eq!(
        reorder_swaps(&[1, 2, 3, 4, 5], 4, 2),
        Some(vec![(4, 3), (3, 2)])
    );
}

#[test]
fn moves_right() {
    assert_eq!(
        reorder_swaps(&[1, 2, 3, 4, 5], 2, 4),
        Some(vec![(2, 3), (3, 4)])
    );
}

#[test]
fn adjacent_is_a_single_swap() {
    assert_eq!(reorder_swaps(&[1, 2, 3], 2, 3), Some(vec![(2, 3)]));
}

#[test]
fn respects_gaps() {
    assert_eq!(
        reorder_swaps(&[1, 2, 5, 7], 7, 2),
        Some(vec![(7, 5), (5, 2)])
    );
}

#[test]
fn unsorted_input_is_fine() {
    assert_eq!(reorder_swaps(&[3, 1, 2], 1, 3), Some(vec![(1, 2), (2, 3)]));
}

#[test]
fn same_index_is_a_no_op() {
    assert_eq!(reorder_swaps(&[1, 2, 3], 2, 2), Some(vec![]))
}

#[test]
fn missing_index_is_none() {
    assert_eq!(reorder_swaps(&[1, 2, 3], 9, 1), None);
    assert_eq!(reorder_swaps(&[1, 2, 3], 1, 9), None);
}
