use std::path::PathBuf;

use sv_parser::{Locate, RefNode, SyntaxTree, WhiteSpace};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Position {
    pub row: u32,
    pub col: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentPosition {
    pub document: PathBuf,
    pub position: Position,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Range {
    pub begin: Position,
    pub end: Position,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentRange {
    pub document: PathBuf,
    pub range: Range,
}

// Searching item by location
impl PartialEq<Position> for Range {
    fn eq(&self, other: &Position) -> bool {
        (self.begin.row == other.row && self.begin.col >= other.col)
            || (self.begin.row > other.row && self.end.row < other.row)
            || (self.end.row == other.row && self.end.col <= other.col)
    }
}

impl PartialOrd<Position> for Range {
    fn partial_cmp(&self, other: &Position) -> Option<std::cmp::Ordering> {
        use std::cmp::Ordering;

        if self.end.row < other.row {
            return Some(Ordering::Less);
        }
        if self.begin.row > other.row {
            return Some(Ordering::Greater);
        }
        if self.begin.row == other.row {
            if self.begin.col < other.col {
                return Some(Ordering::Less);
            }
        }
        if self.end.row == other.row {
            if self.end.col < other.col {
                return Some(Ordering::Greater);
            }
        }

        Some(Ordering::Equal)
    }
}

pub struct LineIndex(Vec<usize>);

impl LineIndex {
    pub fn new(syntax_tree: &SyntaxTree) -> Self {
        let mut offsets = vec![0];
        for node in syntax_tree {
            match node {
                RefNode::WhiteSpace(WhiteSpace::Newline(locate)) => {
                    let newline_str = syntax_tree.get_str(locate).unwrap();
                    let mut pos = locate.offset;
                    let mut first = true;
                    for s in newline_str.split("\r\n") {
                        for s in s.split("\n") {
                            if !first {
                                offsets.push(pos);
                            } else {
                                first = false;
                            }
                            pos += s.len();
                            pos += 1;
                        }
                        pos += 1;
                    }
                }
                _ => {}
            }
        }

        Self(offsets)
    }

    pub fn locate_to_position(&self, locate: &Locate) -> Position {
        let ret =
            Position {
                row: locate.line - 1,
                col: (locate.offset
                    - self.0.get(locate.line as usize - 1).unwrap_or_else(|| {
                        panic!("line_index mismatched at line: {}", locate.line)
                    })) as _,
            };
        ret
    }

    pub fn offset_to_position(&self, offset: usize) -> Position {
        let mut col = 0;
        let mut line = 0;
        for (idx, accumulated) in self.0.iter().enumerate() {
            line = idx;
            if *accumulated > offset {
                break;
            }
            col = offset - accumulated;
        }

        Position {
            row: (line - 1) as _,
            col: col as _,
        }
    }
}
