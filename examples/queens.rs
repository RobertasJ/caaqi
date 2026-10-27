//! Backtracking without a search stack: eight queens that no queen attacks.
//!
//! The program places a queen on each row, top to bottom, in the first
//! column no queen above attacks. A row starts looking at a column it reads
//! from a value, 0 at first. When a row has no column left, the queen above
//! it is misplaced, so the row writes into the past, at the row above's
//! point, that it should start one column further.
//!
//! That write rewrites everything it affects. The rows below observed the
//! queen above, so they're rewritten, and so are the dead ends that pushed
//! them further, since affected code made them. So the rows below start
//! again at column 0, as if the queen above had been placed differently all
//! along.
//!
//! So the program is a straight line, with no backtracking in it: the
//! machine reruns it until no row reaches a dead end.
//!
//! Run with `just example queens`.

use caaqi::prelude::*;

const SIZE: usize = 8;

/// A placed queen: its column, and how its row can be told to start
/// further.
struct Queen {
    column: usize,
    start: Value<usize>,
    row_point: Time,
}

/// Places a queen on every row, or returns `None` if there's no way to.
fn place_queens() -> Option<Vec<usize>> {
    let mut queens: Vec<Queen> = Vec::new();
    for row in 0..SIZE {
        let start = create_value(0);
        let row_point = now();
        // Read after `row_point`, so it sees a start written there.
        let free = (start.get()..SIZE).find(|&column| {
            queens.iter().enumerate().all(|(above, queen)| {
                queen.column != column && queen.column.abs_diff(column) != row - above
            })
        });
        match free {
            Some(column) => queens.push(Queen {
                column,
                start,
                row_point,
            }),
            None => {
                // A dead end: the queen above moves one column on. With no
                // queen above, there's no way to place them all.
                let above = queens.last()?;
                above.start.write_at(above.row_point, above.column + 1);
            }
        }
    }
    Some(queens.iter().map(|queen| queen.column).collect())
}

fn main() {
    let (mut machine, board) = new_machine(place_queens);
    let runs = machine.run().unwrap();

    let board = scope(&mut machine, || board.get()).expect("eight queens can be placed");
    for column in board {
        let row: Vec<_> = (0..SIZE)
            .map(|square| if square == column { "♛" } else { "·" })
            .collect();
        println!("{}", row.join(" "));
    }
    println!(
        "\nplaced in a straight line, settled after {runs} runs: one per dead end, and the last"
    );
}
