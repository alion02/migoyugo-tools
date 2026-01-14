//! Integration test with testgame.txt

use myu_core::*;

#[test]
fn test_parse_testgame() {
    let content = include_str!("testgame.txt");
    let (game, fmt) = parse_game(content, true).expect("Failed to parse testgame.txt");

    // The game has 39 moves (20 move pairs, minus black's last move)
    assert_eq!(game.len(), 39, "Expected 39 moves");

    // The format should be detected as YugosParens
    assert_eq!(fmt, Some(MvFormat::YugosParens), "Expected YugosParens format");

    // Verify specific moves
    assert_eq!(format_sq(game.moves()[0].sq), "d4"); // 1. d4
    assert_eq!(format_sq(game.moves()[1].sq), "c3"); // 1. ... c3

    // Verify yugo formation on move 5 (white): d6 (1 yugo) - index 8
    let mv8 = game.moves()[8];
    assert_eq!(format_sq(mv8.sq), "d6");
    assert_eq!(mv8.yugos_formed, 1);

    // Verify move 19 (black): e4 (2 yugos) - index 37
    let mv37 = game.moves()[37];
    assert_eq!(format_sq(mv37.sq), "e4");
    assert_eq!(mv37.yugos_formed, 2);

    // Verify the last move (white 20): a8 (1 yugo) - index 38
    let last = game.moves()[38];
    assert_eq!(format_sq(last.sq), "a8");
    assert_eq!(last.yugos_formed, 1);

    // Print final state for debugging
    let state = game.current_state();
    println!("Final state: {}", format_state(state));
    println!("Outcome: {:?}", state.outcome());
    println!("White score: {}, Black score: {}", state.score(Color::White), state.score(Color::Black));
}

#[test]
fn test_roundtrip_testgame() {
    let content = include_str!("testgame.txt");
    let (game, _) = parse_game(content, true).expect("Failed to parse testgame.txt");

    // Format without numbers
    let fmt = PgnFormat { move_numbers: false, newlines: false, mv_format: MvFormat::Plain };
    let formatted = format_game(&game, &fmt, false, false);

    // Re-parse the formatted output
    let (game2, _) = parse_game(&formatted, false).expect("Failed to re-parse");

    // Verify they're the same
    assert_eq!(game.len(), game2.len());
    for (m1, m2) in game.moves().iter().zip(game2.moves()) {
        assert_eq!(m1.sq, m2.sq);
    }
}
