use std::fmt;
use std::fmt::Formatter;
use std::hash::{Hash, Hasher};
use std::str::FromStr;

use crate::bitboard::BitBoard;
use crate::castling_rights::{CastlingRights, UPDATE_CASTLING_RIGHT_TABLE};
use crate::chess_move::Move;
use crate::chess_move::MoveFlag::{Capture, Castling, DoublePawnPush, EnPassant};
use crate::color::{Color, ALL_COLORS, NUM_COLORS};
use crate::movgen::{calculate_pinned_checkers_pinners, generate_moves, MoveList};
use crate::piece::{Piece, ALL_PIECES, NUM_PIECES};
use crate::square::{File, Square};
use crate::tables::{
    between, get_bishop_attacks, get_knight_attacks, get_pawn_attacks, get_rook_attacks,
};
use crate::uci_move::UCIMove;
use crate::zobrist::{CASTLE_KEYS, EN_PASSANT_KEYS, PIECE_KEYS, SIDE_KEY};

#[derive(Debug, Eq, PartialEq)]
pub enum BoardStatus {
    Ongoing,
    Stalemate,
    Checkmate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoardState {
    hash: u64,
    en_passant_target: Option<Square>,
    castling_rights: CastlingRights,
    rule50: u8,
    ply: u16,
    checkers: BitBoard,
    pinned: BitBoard,
    last_move: Option<Move>,
    captured_piece: Option<Piece>,
}

#[derive(Debug, Clone, Eq)]
pub struct Board {
    pieces: [BitBoard; NUM_PIECES],
    occupancies: [BitBoard; NUM_COLORS],
    combined: BitBoard,
    side_to_move: Color,
    pub history: Vec<BoardState>,
    state: BoardState,
    game_ply: u16,
}

impl PartialEq for Board {
    fn eq(&self, other: &Self) -> bool {
        self.pieces == other.pieces
            && self.occupancies == other.occupancies
            && self.combined == other.combined
            && self.side_to_move == other.side_to_move
    }
}

impl Default for Board {
    fn default() -> Self {
        Board::STARTING_POS_FEN.parse().unwrap()
    }
}

impl Board {
    pub const STARTING_POS_FEN: &'static str =
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

    pub const KILLER_POS_FEN: &'static str =
        "rnbqkb1r/pp1p1pPp/8/2p1pP2/1P1P4/3P3P/P1P1P3/RNBQKBNR w KQkq e6 0 1";

    pub fn piece_on_square(&self, square: Square) -> Option<Piece> {
        let bitboard = BitBoard::from_square(square);
        if (self.combined & bitboard) == BitBoard(0) {
            None
        } else {
            for piece in ALL_PIECES {
                if (self.pieces[piece as usize] & bitboard) != BitBoard(0) {
                    return Some(piece);
                }
            }
            None
        }
    }

    pub fn color_on_square(&self, square: Square) -> Option<Color> {
        let bitboard = BitBoard::from_square(square);
        if (self.occupancies[Color::White as usize] & bitboard) != BitBoard(0) {
            Some(Color::White)
        } else if (self.occupancies[Color::Black as usize] & bitboard) != BitBoard(0) {
            Some(Color::Black)
        } else {
            None
        }
    }

    pub fn pieces(&self, piece: Piece) -> BitBoard {
        self.pieces[piece as usize]
    }

    pub fn occupancies(&self, color: Color) -> BitBoard {
        self.occupancies[color as usize]
    }

    pub fn combined(&self) -> BitBoard {
        self.combined
    }

    pub fn side_to_move(&self) -> Color {
        self.side_to_move
    }

    pub fn checkers(&self) -> BitBoard {
        self.state.checkers
    }

    pub fn pinned(&self) -> BitBoard {
        self.state.pinned
    }

    pub fn castling_rights(&self) -> CastlingRights {
        self.state.castling_rights
    }

    pub fn en_passant_target(&self) -> Option<Square> {
        self.state.en_passant_target
    }

    pub fn apply_uci_move(&mut self, uci_move: UCIMove) {
        let chess_move = generate_moves(self)
            .into_iter()
            .find(|m| uci_move == m)
            .unwrap();
        self.apply_move(chess_move);
    }

    pub fn apply_move(&mut self, mov: Move) {
        // copy state and put it in
        let mut new_state = self.state.clone();
        // new_state.previous = Some(self.state.clone());

        new_state.last_move = Some(mov);

        self.game_ply += 1;
        new_state.ply += 1;
        new_state.rule50 += 1;

        // en passant is cleared after doing any move
        new_state.en_passant_target = None;
        if let Some(en_passant_target) = self.en_passant_target() {
            new_state.hash ^= EN_PASSANT_KEYS[en_passant_target.to_file() as usize];
        }

        new_state.captured_piece = None;

        // get piece at target square before moving
        let target_piece = self.piece_on_square(mov.to);

        // remove piece from from
        self.pieces[mov.piece as usize] ^= mov.from;
        self.occupancies[self.side_to_move as usize] ^= mov.from;
        self.combined ^= mov.from;
        new_state.hash ^=
            PIECE_KEYS[self.side_to_move as usize][mov.piece as usize][mov.from as usize];

        // set piece in to
        self.pieces[mov.piece as usize] |= mov.to;
        self.occupancies[self.side_to_move as usize] |= mov.to;
        self.combined |= mov.to;
        new_state.hash ^=
            PIECE_KEYS[self.side_to_move as usize][mov.piece as usize][mov.to as usize];

        if mov.flags == Capture {
            // replace opponents piece with your own
            // get piece that was at the target square before the move
            let target_piece = target_piece.expect("captures require a piece on the target square");

            new_state.captured_piece = Some(target_piece);

            if target_piece != mov.piece {
                self.pieces[target_piece as usize] ^= mov.to;
            }
            self.occupancies[!self.side_to_move as usize] ^= mov.to;
            new_state.hash ^=
                PIECE_KEYS[!self.side_to_move as usize][target_piece as usize][mov.to as usize];

            // combined is unchanged here

            // remove castling right for that side
            if target_piece == Piece::Rook {
                // remove castling rights from hash
                new_state.hash ^= CASTLE_KEYS[new_state.castling_rights.to_usize()];

                new_state.castling_rights &= UPDATE_CASTLING_RIGHT_TABLE[mov.from as usize];
                new_state.castling_rights &= UPDATE_CASTLING_RIGHT_TABLE[mov.to as usize];

                // add castling rights to hash
                new_state.hash ^= CASTLE_KEYS[new_state.castling_rights.to_usize()];
            }

            new_state.rule50 = 0;
        }

        if let Some(promotion) = mov.promotion {
            // remove old piece type
            self.pieces[mov.piece as usize] ^= mov.to;
            // add to new piece type
            self.pieces[promotion.as_piece() as usize] |= mov.to;

            new_state.hash ^=
                PIECE_KEYS[self.side_to_move as usize][mov.piece as usize][mov.to as usize];
            new_state.hash ^= PIECE_KEYS[self.side_to_move as usize][promotion.as_piece() as usize]
                [mov.to as usize];
        }

        if mov.flags == DoublePawnPush {
            // update en_passant_target when double pushing
            new_state.en_passant_target = Some(mov.to.forward(!self.side_to_move).unwrap());

            new_state.hash ^= EN_PASSANT_KEYS[mov.to.to_file() as usize];
        }

        if mov.flags == EnPassant {
            let capture_piece = mov.to.forward(!self.side_to_move).unwrap();
            self.pieces[Piece::Pawn as usize] ^= capture_piece;
            self.occupancies[!self.side_to_move as usize] ^= capture_piece;
            self.combined ^= capture_piece;

            new_state.hash ^= PIECE_KEYS[!self.side_to_move as usize][Piece::Pawn as usize]
                [capture_piece as usize];
        }

        const CASTLE_CONFIG: [(File, File); 2] = [(File::A, File::D), (File::H, File::F)];

        if mov.flags == Castling {
            let backrank = self.side_to_move.backrank();
            let target_file = mov.to.to_file();
            let (rook_start_file, rook_end_file) = CASTLE_CONFIG[target_file as usize / 4];
            let (rook_start_square, rook_end_square) = (
                Square::from(backrank, rook_start_file),
                Square::from(backrank, rook_end_file),
            );

            // remove piece from from
            self.pieces[Piece::Rook as usize] ^= rook_start_square;
            self.occupancies[self.side_to_move as usize] ^= rook_start_square;
            self.combined ^= rook_start_square;
            new_state.hash ^= PIECE_KEYS[self.side_to_move as usize][Piece::Rook as usize]
                [rook_start_square as usize];

            // set piece in to
            self.pieces[Piece::Rook as usize] |= rook_end_square;
            self.occupancies[self.side_to_move as usize] |= rook_end_square;
            self.combined |= rook_end_square;
            new_state.hash ^= PIECE_KEYS[self.side_to_move as usize][Piece::Rook as usize]
                [rook_end_square as usize];
        }

        if mov.piece == Piece::Pawn {
            new_state.rule50 = 0;
        }

        // update castling rights
        if mov.piece == Piece::Rook {
            // rook moved
            new_state.hash ^= CASTLE_KEYS[new_state.castling_rights.to_usize()];
            new_state.castling_rights &= UPDATE_CASTLING_RIGHT_TABLE[mov.from as usize];
            new_state.castling_rights &= UPDATE_CASTLING_RIGHT_TABLE[mov.to as usize];
            new_state.hash ^= CASTLE_KEYS[new_state.castling_rights.to_usize()];
        } else if mov.piece == Piece::King {
            // remove castling rights for side if king moved (includes castling)
            new_state.hash ^= CASTLE_KEYS[new_state.castling_rights.to_usize()];
            new_state.castling_rights -= match self.side_to_move {
                Color::White => CastlingRights::WHITE_BOTH_SIDES,
                Color::Black => CastlingRights::BLACK_BOTH_SIDES,
            };
            new_state.hash ^= CASTLE_KEYS[new_state.castling_rights.to_usize()];
        }

        // update side
        self.side_to_move = !self.side_to_move;
        new_state.hash ^= SIDE_KEY;

        // TODO: update incrementally instead
        let king_square =
            (self.pieces(Piece::King) & self.occupancies(self.side_to_move())).bit_scan();

        let mut potential_pinners = BitBoard(0);
        let mut pinned = BitBoard(0);

        let mut checkers = BitBoard(0);

        // pretend king is a bishop and see if any other bishop OR queen is attacked by that
        potential_pinners |= get_bishop_attacks(king_square, BitBoard(0))
            & (self.pieces(Piece::Bishop) | self.pieces(Piece::Queen));

        // now pretend the king is a rook and so the same procedure
        potential_pinners |= get_rook_attacks(king_square, BitBoard(0))
            & (self.pieces(Piece::Rook) | self.pieces(Piece::Queen));

        // limit to opponent's pieces
        potential_pinners &= self.occupancies(!self.side_to_move());

        let mut pinners = BitBoard(0);

        for square in potential_pinners.iter() {
            let potentially_pinned = between(square, king_square) & self.combined();
            if potentially_pinned.is_empty() {
                checkers |= square;
            } else if potentially_pinned.popcnt() == 1 {
                pinned |= potentially_pinned;
                pinners |= potential_pinners;
            }
        }

        // TODO: only update when knight moved
        // now pretend the king is a knight and check if it attacks an enemy knight
        checkers |= get_knight_attacks(king_square)
            & self.pieces(Piece::Knight)
            & self.occupancies(!self.side_to_move());

        // TODO: only update when pawn moved
        // do the same thing for pawns
        checkers |= get_pawn_attacks(king_square, self.side_to_move())
            & self.pieces(Piece::Pawn)
            & self.occupancies(!self.side_to_move());

        // update pinned, checkers
        new_state.pinned = pinned;
        new_state.checkers = checkers;

        let old_state = std::mem::replace(&mut self.state, new_state);
        self.history.push(old_state);
    }

    pub fn undo_move(&mut self) {
        // revert last move from popped state
        if let Some(last_move) = self.state.last_move {
            self.side_to_move = !self.side_to_move;
            const CASTLE_CONFIG: [(File, File); 2] = [(File::A, File::D), (File::H, File::F)];

            if last_move.flags == Castling {
                let backrank = self.side_to_move.backrank();
                let target_file = last_move.to.to_file();
                let (rook_start_file, rook_end_file) = CASTLE_CONFIG[target_file as usize / 4];
                let (rook_start_square, rook_end_square) = (
                    Square::from(backrank, rook_start_file),
                    Square::from(backrank, rook_end_file),
                );

                self.pieces[Piece::Rook as usize] |= rook_start_square;
                self.occupancies[self.side_to_move as usize] |= rook_start_square;
                self.combined |= rook_start_square;

                self.pieces[Piece::Rook as usize] ^= rook_end_square;
                self.occupancies[self.side_to_move as usize] ^= rook_end_square;
                self.combined ^= rook_end_square;
            }

            // undo promotion
            if let Some(promotion) = last_move.promotion {
                // remove old piece type
                self.pieces[last_move.piece as usize] |= last_move.to;
                // add to new piece type
                self.pieces[promotion.as_piece() as usize] ^= last_move.to;
            }

            self.pieces[last_move.piece as usize] |= last_move.from;
            self.occupancies[self.side_to_move as usize] |= last_move.from;
            self.combined |= last_move.from;

            self.pieces[last_move.piece as usize] ^= last_move.to;
            self.occupancies[self.side_to_move as usize] ^= last_move.to;
            self.combined ^= last_move.to;

            // undo capture
            if let Some(captured_piece) = self.state.captured_piece {
                self.pieces[captured_piece as usize] |= last_move.to;
                self.occupancies[!self.side_to_move as usize] |= last_move.to;
                self.combined |= last_move.to;
            }

            if last_move.flags == EnPassant {
                let capture_piece = last_move.to.forward(!self.side_to_move).unwrap();
                self.pieces[Piece::Pawn as usize] |= capture_piece;
                self.occupancies[!self.side_to_move as usize] |= capture_piece;
                self.combined |= capture_piece;
            }
        }

        self.game_ply -= 1;

        if let Some(previous_state) = self.history.pop() {
            self.state = previous_state;
        }
    }

    pub fn game_ply(&self) -> u16 {
        self.game_ply
    }

    pub fn generate_moves(&self) -> MoveList {
        generate_moves(self)
    }

    pub fn status(&self) -> BoardStatus {
        // inefficient but works for now
        // should not be used in the search
        let moves = generate_moves(self);
        if moves.is_empty() {
            return if self.state.checkers.is_empty() {
                BoardStatus::Stalemate
            } else {
                BoardStatus::Checkmate
            };
        }
        BoardStatus::Ongoing
    }

    pub fn is_repetition(&self) -> bool {
        let key = self.state.hash;

        return self.history.iter().filter(|c| key == c.hash).count() >= 1;
    }

    #[inline]
    pub fn hash(&self) -> u64 {
        self.state.hash
    }
}

impl Hash for Board {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(self.state.hash);
    }
}

impl fmt::Display for Board {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln!(f)?;
        for rank in (0..8).rev() {
            write!(f, "{}   ", rank + 1)?;
            for file in 0..8 {
                let square = Square::from_index(rank * 8 + file);
                let symbol = if let Some(piece) = self.piece_on_square(square) {
                    let color = self.color_on_square(square).ok_or(fmt::Error)?;
                    piece.to_ascii(color)
                } else {
                    '.'
                };
                write!(f, "{} ", symbol)?;
            }
            writeln!(f)?;
        }
        write!(f, "\n    ")?;
        for file in 'a'..='h' {
            write!(f, "{} ", file)?;
        }

        writeln!(f, "\n")?;
        writeln!(f, "En passant square:\t{:?}", self.state.en_passant_target)?;
        writeln!(f, "Side to move:\t\t{:?}", self.side_to_move)?;
        writeln!(f, "Castling rights:\t{}", self.state.castling_rights)?;
        writeln!(f, "Captured piece:\t{:?}", self.state.captured_piece)?;
        writeln!(f, "Last move:\t{:?}", self.state.last_move)?;
        writeln!(f, "Hash: \t{:#018x}", self.state.hash)?;
        Ok(())
    }
}

#[derive(Debug)]
pub enum ParseFenError {
    PartsMissing,
    BadPlacement,
    NoSuchSide,
    BadCastlingRights,
    BadFullMoveNumber,
    BadHalfMoveClock,
    BadEnPassant,
    TooManyPiecesInRank,
    InvalidPiece,
}

impl FromStr for Board {
    type Err = ParseFenError;

    fn from_str(fen: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = fen.split(' ').collect();
        if parts.len() != 6 {
            return Err(ParseFenError::PartsMissing);
        }

        let (
            placement,
            side_to_move,
            castling_rights,
            en_passant_target,
            halfmove_clock,
            fullmove_number,
        ) = (parts[0], parts[1], parts[2], parts[3], parts[4], parts[5]);

        let ranks_str: Vec<&str> = placement.split('/').collect();
        if ranks_str.len() != 8 {
            return Err(ParseFenError::BadPlacement);
        }

        let mut pieces = [BitBoard(0); NUM_PIECES];
        let mut occupancies = [BitBoard(0); NUM_COLORS];
        let mut combined = BitBoard(0);

        // reverse iterator to start with rank 1
        for (rank, rank_pieces) in (0u8..).zip(ranks_str.iter().rev()) {
            let mut file: u8 = 0;
            for piece_char in rank_pieces.chars() {
                match piece_char.to_digit(10) {
                    // blanks
                    Some(n) => {
                        file += n as u8;
                        if file > 8 {
                            return Err(ParseFenError::TooManyPiecesInRank);
                        }
                    }
                    // piece
                    None => {
                        let color = if piece_char.is_uppercase() {
                            Color::White
                        } else {
                            Color::Black
                        };

                        let piece = match piece_char.to_ascii_lowercase() {
                            'p' => Piece::Pawn,
                            'n' => Piece::Knight,
                            'b' => Piece::Bishop,
                            'r' => Piece::Rook,
                            'q' => Piece::Queen,
                            'k' => Piece::King,
                            _ => return Err(ParseFenError::InvalidPiece),
                        };

                        let square = Square::from_index(rank * 8 + file);

                        // place piece
                        pieces[piece as usize] |= square;
                        occupancies[color as usize] |= square;
                        combined |= square;

                        file += 1;
                    }
                }
            }
        }

        let side_to_move = match side_to_move {
            "w" => Ok(Color::White),
            "b" => Ok(Color::Black),
            _ => Err(ParseFenError::NoSuchSide),
        }?;

        let castling_rights = castling_rights
            .parse::<CastlingRights>()
            .map_err(|_| ParseFenError::BadCastlingRights)?;

        let en_passant_target = match en_passant_target {
            "-" => None,
            target => Some(
                target
                    .parse::<Square>()
                    .map_err(|_| ParseFenError::BadEnPassant)?,
            ),
        };

        let halfmove_clock = halfmove_clock
            .parse::<u8>()
            .map_err(|_| ParseFenError::BadHalfMoveClock)?;

        let fullmove_number = fullmove_number
            .parse::<u16>()
            .map_err(|_| ParseFenError::BadFullMoveNumber)?;

        let partial_board = PartialBoard {
            pieces,
            occupancies,
            combined,
            side_to_move,
            en_passant_target,
            castling_rights,
        };

        let (pinned, checkers, _) = calculate_pinned_checkers_pinners(&partial_board);

        let board = Board {
            pieces,
            occupancies,
            combined,
            side_to_move,
            state: BoardState {
                hash: partial_board.generate_hash_key(),
                en_passant_target,
                castling_rights,
                rule50: halfmove_clock,
                ply: 0,
                checkers,
                pinned,
                last_move: None,
                captured_piece: None,
            },
            history: vec![],
            game_ply: (2 * (fullmove_number - 1)).max(0) + [0, 1][side_to_move as usize],
        };

        // TODO: check if board is sane

        Ok(board)
    }
}

pub struct PartialBoard {
    pieces: [BitBoard; NUM_PIECES],
    occupancies: [BitBoard; NUM_COLORS],
    en_passant_target: Option<Square>,
    castling_rights: CastlingRights,
    combined: BitBoard,
    side_to_move: Color,
}

impl PartialBoard {
    pub fn pieces(&self, piece: Piece) -> BitBoard {
        self.pieces[piece as usize]
    }

    pub fn occupancies(&self, color: Color) -> BitBoard {
        self.occupancies[color as usize]
    }

    pub fn combined(&self) -> BitBoard {
        self.combined
    }

    pub fn side_to_move(&self) -> Color {
        self.side_to_move
    }

    fn generate_hash_key(&self) -> u64 {
        let mut key = 0;

        for color in ALL_COLORS {
            for piece in ALL_PIECES {
                let piece_bitboard = self.pieces(piece) & self.occupancies(color);

                for square in piece_bitboard.iter() {
                    key ^= PIECE_KEYS[color as usize][piece as usize][square as usize];
                }
            }
        }

        if let Some(en_passant_target) = self.en_passant_target {
            key ^= EN_PASSANT_KEYS[en_passant_target.to_file() as usize];
        }

        key ^= CASTLE_KEYS[self.castling_rights.to_usize()];

        if self.side_to_move == Color::Black {
            key ^= SIDE_KEY;
        }

        key
    }
}

#[cfg(test)]
mod test {
    use std::str::FromStr;

    use crate::board::Board;
    use crate::color::Color;
    use crate::piece::Piece;
    use crate::square::Square;

    #[test]
    fn test_display() {
        let expected = "
8   r n b q k b n r 
7   p p p p p p p p 
6   . . . . . . . . 
5   . . . . . . . . 
4   . . . . . . . . 
3   . . . . . . . . 
2   P P P P P P P P 
1   R N B Q K B N R 

    a b c d e f g h 

En passant square:	None
Side to move:		White
Castling rights:	KQkq
Captured piece:	None
Last move:	None
Hash: 	0x4a887e3c9bc2624a
";
        let board = Board::default();
        println!("{}", board);
        assert_eq!(expected, board.to_string());
    }

    #[test]
    fn test_fen_parsing() {
        let board = Board::from_str("2r5/8/8/3R4/2P1k3/2K5/8/8 b - - 0 1").unwrap();

        assert_eq!(board.piece_on_square(Square::C3), Some(Piece::King));
        assert_eq!(board.piece_on_square(Square::E4), Some(Piece::King));
        assert_eq!(board.piece_on_square(Square::C4), Some(Piece::Pawn));
        assert_eq!(board.piece_on_square(Square::D5), Some(Piece::Rook));
        assert_eq!(board.piece_on_square(Square::C8), Some(Piece::Rook));

        assert_eq!(board.color_on_square(Square::C3), Some(Color::White));
        assert_eq!(board.color_on_square(Square::E4), Some(Color::Black));
        assert_eq!(board.color_on_square(Square::C4), Some(Color::White));
        assert_eq!(board.color_on_square(Square::D5), Some(Color::White));
        assert_eq!(board.color_on_square(Square::C8), Some(Color::Black));

        println!("{board}");
    }
}
