use crate::bitboard::BitBoard;
use crate::board::Board;
use crate::chess_move::{Move, MoveFlag};
use crate::movgen::{MoveList, PieceMoveGenerator};
use crate::piece::Piece;
use crate::square::Square;
use crate::tables::{get_bishop_attacks, get_pawn_attacks, get_rook_attacks};

pub struct EnPassantMoveGenerator;

impl EnPassantMoveGenerator {
    pub fn valid_ep(board: &Board, capture: Square, source: Square, destination: Square) -> bool {
        // create combined bitboard of board with both source and capture removed.
        // removing the squares simulates the move
        let combined =
            board.combined() & !BitBoard::from_square(capture) & !BitBoard::from_square(source)
                | BitBoard::from_square(destination);

        let king_square =
            (board.pieces(Piece::King) & board.occupancies(board.side_to_move())).bit_scan();

        let mut attack = BitBoard(0);

        // pretend like the king is a rook
        attack |= get_rook_attacks(king_square, combined)
            & (board.pieces(Piece::Rook) | board.pieces(Piece::Queen))
            & board.occupancies(!board.side_to_move());

        // pretend like the king is a bishop
        attack |= get_bishop_attacks(king_square, combined)
            & (board.pieces(Piece::Bishop) | board.pieces(Piece::Queen))
            & board.occupancies(!board.side_to_move());

        attack == BitBoard::EMPTY
    }
}

impl PieceMoveGenerator for EnPassantMoveGenerator {
    fn generate<const CHECK: bool>(board: &Board, move_list: &mut MoveList) {
        if let Some(ep_square) = board.en_passant_target() {
            let side_to_move = board.side_to_move();
            let current_sides_pawns = board.pieces(Piece::Pawn) & board.occupancies(side_to_move);

            for source in current_sides_pawns.iter() {
                let attack =
                    get_pawn_attacks(source, side_to_move) & BitBoard::from_square(ep_square);

                for destination in attack.iter() {
                    let capture = destination.forward(!side_to_move).unwrap();

                    if Self::valid_ep(board, capture, source, destination) {
                        move_list.push(Move {
                            from: source,
                            to: destination,
                            promotion: None,
                            piece: Piece::Pawn,
                            flags: MoveFlag::EnPassant,
                        });
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use std::str::FromStr;

    use crate::board::Board;
    use crate::chess_move::{Move, MoveFlag};
    use crate::movgen::en_passant::EnPassantMoveGenerator;
    use crate::movgen::{MoveList, PieceMoveGenerator};
    use crate::piece::Piece;
    use crate::square::Square;

    #[test]
    fn test_en_passant() {
        let board = Board::from_str("8/8/k7/8/2Pp4/8/8/3K4 b - c3 0 1").unwrap();
        let mut move_list = MoveList::new();
        EnPassantMoveGenerator::generate::<false>(&board, &mut move_list);
        println!("{:#?}", move_list);

        assert_eq!(move_list.len(), 1);
        assert!(move_list.contains(&Move {
            from: Square::D4,
            to: Square::C3,
            promotion: None,
            piece: Piece::Pawn,
            flags: MoveFlag::EnPassant,
        }));
    }

    #[test]
    fn test_invalid_en_passant_horizontal() {
        let board = Board::from_str("8/8/8/8/k1Pp3R/8/8/3K4 b - c3 0 1").unwrap();
        let mut move_list = MoveList::new();
        EnPassantMoveGenerator::generate::<false>(&board, &mut move_list);
        println!("{:#?}", move_list);

        assert_eq!(move_list.len(), 0);
    }

    #[test]
    fn test_invalid_en_passant_vertical() {
        let board = Board::from_str("5q2/8/8/4pP2/8/8/8/5K2 w - e6 0 1").unwrap();
        let mut move_list = MoveList::new();
        EnPassantMoveGenerator::generate::<false>(&board, &mut move_list);
        println!("{:#?}", move_list);

        assert_eq!(move_list.len(), 0);
    }

    #[test]
    fn test_invalid_en_passant_diagonal() {
        let board = Board::from_str("8/7q/8/4pP2/8/8/8/1K6 w - e6 0 1").unwrap();
        let mut move_list = MoveList::new();
        EnPassantMoveGenerator::generate::<false>(&board, &mut move_list);
        println!("{:#?}", move_list);

        assert_eq!(move_list.len(), 0);
    }

    #[test]
    fn test_valid_en_passant() {
        let board = Board::from_str("8/7k/8/8/2Pp4/8/8/K7 b - c3 0 1").unwrap();
        let mut move_list = MoveList::new();
        EnPassantMoveGenerator::generate::<false>(&board, &mut move_list);
        println!("{:#?}", move_list);

        assert_eq!(move_list.len(), 1);
        assert!(move_list.contains(&Move {
            from: Square::D4,
            to: Square::C3,
            promotion: None,
            piece: Piece::Pawn,
            flags: MoveFlag::EnPassant,
        }));
    }

    #[test]
    fn test_en_passant_edge() {
        let board = Board::from_str(
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/Pp2P3/2N2Q1p/1PPBBPPP/R3K2R b KQkq a3 0 1",
        )
        .unwrap();
        let mut move_list = MoveList::new();
        EnPassantMoveGenerator::generate::<false>(&board, &mut move_list);
        println!("{:#?}", move_list);

        assert_eq!(move_list.len(), 1);
        assert!(move_list.contains(&Move {
            from: Square::B4,
            to: Square::A3,
            promotion: None,
            piece: Piece::Pawn,
            flags: MoveFlag::EnPassant,
        }));
    }

    #[test]
    fn test_en_passant_in_check() {
        let board = Board::from_str("1kb5/p7/P7/2Ppb2B/7P/7K/8/8 w - d6 0 4").unwrap();
        let mut move_list = MoveList::new();
        EnPassantMoveGenerator::generate::<true>(&board, &mut move_list);
        println!("{:#?}", move_list);

        assert_eq!(move_list.len(), 0);
    }

    #[test]
    fn test_en_passant2() {
        let board = Board::from_str("8/8/3p4/1Pp4r/KR3p1k/8/4P1P1/8 w - c6 0 2").unwrap();
        let mut move_list = MoveList::new();
        EnPassantMoveGenerator::generate::<true>(&board, &mut move_list);
        println!("{:#?}", move_list);

        assert_eq!(move_list.len(), 1);
    }

    #[test]
    fn test_en_passant_vertical_pin() {
        let board = Board::from_str("8/8/3p4/KPp3kr/5pP1/8/4P3/6R1 b - g3 0 3").unwrap();
        let mut move_list = MoveList::new();
        EnPassantMoveGenerator::generate::<false>(&board, &mut move_list);
        println!("{:#?}", move_list);

        assert_eq!(move_list.len(), 1);
    }
}
