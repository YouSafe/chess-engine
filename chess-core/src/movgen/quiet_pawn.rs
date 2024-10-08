use crate::bitboard::BitBoard;
use crate::board::Board;
use crate::chess_move::{Move, MoveFlag};
use crate::movgen::{MoveList, PieceMoveGenerator};
use crate::piece::Piece;
use crate::promotion::ALL_PROMOTIONS;
use crate::tables::between;

pub struct QuietPawnMoveGenerator;

impl PieceMoveGenerator for QuietPawnMoveGenerator {
    fn generate<const CHECK: bool>(board: &Board, move_list: &mut MoveList) {
        let mut push_mask = !BitBoard::EMPTY;

        let side_to_move = board.side_to_move();
        let current_sides_pawns = board.pieces(Piece::Pawn) & board.occupancies(side_to_move);

        let pinned = board.pinned();

        let king_square =
            (board.pieces(Piece::King) & board.occupancies(board.side_to_move())).bit_scan();

        if CHECK {
            let checkers = board.checkers();
            let checker = checkers.bit_scan();
            push_mask = between(king_square, checker);
        }

        // determine source squares that can move:
        // they have to either be not pinned or pinned with the king being on the same file
        let movable_sources =
            current_sides_pawns & (!pinned | (pinned & BitBoard::mask_file(king_square.file())));

        let forward_shift: i32 = 8 - 16 * (side_to_move as i32);

        // those sources are then shifted one square forward and any overlaps with existing pieces
        // on the board are removed
        let single_push = movable_sources.shift(forward_shift) & !board.combined();

        // restrict the single push targets to squares they can actually move to (check evasion)
        let single_push_targets = single_push & push_mask;

        // move the already moved squares, remove overlaps and restrict the final target squares to
        // legal squares, respecting checks
        let double_push_targets = single_push.shift(forward_shift)
            & !board.combined()
            & BitBoard::mask_rank(side_to_move.double_pawn_push_rank())
            & push_mask;

        let promotion_rank = BitBoard::mask_rank((!side_to_move).backrank());

        let non_promotions = single_push_targets & !promotion_rank;
        let promotions = single_push_targets & promotion_rank;

        for target in promotions.iter() {
            let source = target.forward(!side_to_move).unwrap();
            for promotion in ALL_PROMOTIONS {
                move_list.push(Move {
                    from: source,
                    to: target,
                    piece: Piece::Pawn,
                    promotion: Some(promotion),
                    flags: MoveFlag::Normal,
                });
            }
        }

        for target in double_push_targets.iter() {
            let source = target
                .forward(!side_to_move)
                .unwrap()
                .forward(!side_to_move)
                .unwrap();

            move_list.push(Move {
                from: source,
                to: target,
                piece: Piece::Pawn,
                promotion: None,
                flags: MoveFlag::DoublePawnPush,
            });
        }

        for target in non_promotions.iter() {
            let source = target.forward(!side_to_move).unwrap();

            move_list.push(Move {
                from: source,
                to: target,
                piece: Piece::Pawn,
                promotion: None,
                flags: MoveFlag::Normal,
            });
        }
    }
}

#[cfg(test)]
mod test {
    use std::str::FromStr;

    use crate::board::Board;
    use crate::chess_move::{Move, MoveFlag};
    use crate::movgen::quiet_pawn::QuietPawnMoveGenerator;
    use crate::movgen::{MoveList, PieceMoveGenerator};
    use crate::piece::Piece;
    use crate::promotion::ALL_PROMOTIONS;
    use crate::square::Square::*;

    #[test]
    fn test_single_and_double_push() {
        let board = Board::from_str("k7/8/8/8/8/8/7P/K7 w - - 0 1").unwrap();
        let mut move_list = MoveList::new();
        QuietPawnMoveGenerator::generate::<false>(&board, &mut move_list);

        println!("{:#?}", move_list);

        assert_eq!(move_list.len(), 2);
        assert!(move_list.contains(&Move {
            from: H2,
            to: H3,
            promotion: None,
            piece: Piece::Pawn,
            flags: MoveFlag::Normal,
        }));

        assert!(move_list.contains(&Move {
            from: H2,
            to: H4,
            promotion: None,
            piece: Piece::Pawn,
            flags: MoveFlag::DoublePawnPush,
        }));
    }

    #[test]
    pub fn test_promotion() {
        let board = Board::from_str("k7/7P/8/8/8/8/8/K7 w - - 0 1").unwrap();
        let mut move_list = MoveList::new();
        QuietPawnMoveGenerator::generate::<false>(&board, &mut move_list);
        println!("{:#?}", move_list);

        assert_eq!(move_list.len(), 4);
        for promotion in ALL_PROMOTIONS {
            assert!(move_list.contains(&Move {
                from: H7,
                to: H8,
                promotion: Some(promotion),
                piece: Piece::Pawn,
                flags: MoveFlag::Normal,
            }));
        }
    }

    #[test]
    pub fn test_forced_check_block() {
        let board = Board::from_str("6k1/8/8/8/K6r/8/4P3/8 w - - 0 1").unwrap();
        let mut move_list = MoveList::new();
        QuietPawnMoveGenerator::generate::<true>(&board, &mut move_list);
        println!("{:#?}", move_list);

        assert_eq!(move_list.len(), 1);

        assert!(move_list.contains(&Move {
            from: E2,
            to: E4,
            promotion: None,
            piece: Piece::Pawn,
            flags: MoveFlag::DoublePawnPush,
        }));
    }

    #[test]
    pub fn test_pinned_by_rook_but_can_move_forward() {
        let board = Board::from_str("1K4k1/8/8/1P6/8/1r6/8/8 w - - 0 1").unwrap();
        let mut move_list = MoveList::new();
        QuietPawnMoveGenerator::generate::<false>(&board, &mut move_list);
        println!("{:#?}", move_list);

        assert_eq!(move_list.len(), 1);
        assert!(move_list.contains(&Move {
            from: B5,
            to: B6,
            promotion: None,
            piece: Piece::Pawn,
            flags: MoveFlag::Normal,
        }));
    }

    #[test]
    pub fn test_rook_backward_pin() {
        let board = Board::from_str("1r4k1/8/8/8/8/8/1P6/1K6 w - - 0 1").unwrap();
        let mut move_list = MoveList::new();
        QuietPawnMoveGenerator::generate::<false>(&board, &mut move_list);
        println!("{:#?}", move_list);

        assert_eq!(move_list.len(), 2);
        assert!(move_list.contains(&Move {
            from: B2,
            to: B3,
            promotion: None,
            piece: Piece::Pawn,
            flags: MoveFlag::Normal,
        }));
        assert!(move_list.contains(&Move {
            from: B2,
            to: B4,
            promotion: None,
            piece: Piece::Pawn,
            flags: MoveFlag::DoublePawnPush,
        }));
    }

    #[test]
    fn test_bishop_pin() {
        let board = Board::from_str("6k1/8/5b2/8/8/8/1P6/K7 w - - 0 1").unwrap();
        let mut move_list = MoveList::new();
        QuietPawnMoveGenerator::generate::<false>(&board, &mut move_list);
        println!("{:#?}", move_list);

        assert_eq!(move_list.len(), 0);
    }

    #[test]
    fn test_two_pawns_one_bishop_pin() {
        let board = Board::from_str("6k1/8/5b2/8/8/1P6/1P6/K7 w - - 0 1").unwrap();
        let mut move_list = MoveList::new();
        QuietPawnMoveGenerator::generate::<false>(&board, &mut move_list);
        println!("{:#?}", move_list);

        assert_eq!(move_list.len(), 1);
        assert!(move_list.contains(&Move {
            from: B3,
            to: B4,
            promotion: None,
            piece: Piece::Pawn,
            flags: MoveFlag::Normal,
        }));
    }

    #[test]
    fn test_check_pawn_can_not_block() {
        let board = Board::from_str("6k1/8/5b2/8/8/1P6/8/K7 w - - 0 1").unwrap();
        let mut move_list = MoveList::new();
        QuietPawnMoveGenerator::generate::<true>(&board, &mut move_list);
        println!("{:#?}", move_list);

        assert_eq!(move_list.len(), 0);
    }

    #[test]
    fn test_pawn_pushes_startpos() {
        let board = Board::default();
        let mut move_list = MoveList::new();
        QuietPawnMoveGenerator::generate::<false>(&board, &mut move_list);
        println!("{:#?}", move_list);

        assert_eq!(move_list.len(), 16);
    }
}
