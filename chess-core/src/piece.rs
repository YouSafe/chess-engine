use crate::color::Color;
use crate::piece::Piece::{Bishop, King, Knight, Pawn, Queen, Rook};

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Piece {
    Pawn,
    Knight,
    Bishop,
    Rook,
    Queen,
    King,
}

pub const NUM_PIECES: usize = 6;
pub const ALL_PIECES: [Piece; 6] = [Pawn, Knight, Bishop, Rook, Queen, King];

impl Piece {
    pub fn to_unicode(&self, color: Color) -> char {
        match (color, *self) {
            (Color::White, Pawn) => '♙',
            (Color::White, Knight) => '♘',
            (Color::White, Bishop) => '♗',
            (Color::White, Rook) => '♖',
            (Color::White, Queen) => '♕',
            (Color::White, King) => '♔',

            (Color::Black, Pawn) => '♟',
            (Color::Black, Knight) => '♞',
            (Color::Black, Bishop) => '♝',
            (Color::Black, Rook) => '♜',
            (Color::Black, Queen) => '♛',
            (Color::Black, King) => '♚',
        }
    }

    pub fn to_ascii(&self, color: Color) -> char {
        match (color, *self) {
            (Color::White, Pawn) => 'P',
            (Color::White, Knight) => 'N',
            (Color::White, Bishop) => 'B',
            (Color::White, Rook) => 'R',
            (Color::White, Queen) => 'Q',
            (Color::White, King) => 'K',

            (Color::Black, Pawn) => 'p',
            (Color::Black, Knight) => 'n',
            (Color::Black, Bishop) => 'b',
            (Color::Black, Rook) => 'r',
            (Color::Black, Queen) => 'q',
            (Color::Black, King) => 'k',
        }
    }
}
