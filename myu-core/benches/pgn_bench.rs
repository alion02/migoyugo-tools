use criterion::{black_box, criterion_group, criterion_main, Criterion};
use myu_core::{parse_game, format_game, PgnFormat, MvFormat};

fn criterion_benchmark(c: &mut Criterion) {
    // A reasonably long game to make formatting cost significant
    let pgn_input = "1. d4 c3 2. c4 e4 3. d5 e3 4. d3 e6 5. d6 e5 6. f4 g5 7. h4 h5 8. f3 g6";
    let (game, _) = parse_game(pgn_input, false).expect("Failed to parse game");

    let fmt = PgnFormat {
        move_numbers: true,
        newlines: false,
        mv_format: MvFormat::Plain,
    };

    c.bench_function("format_game", |b| b.iter(|| {
        format_game(black_box(&game), black_box(&fmt), black_box(false), black_box(false))
    }));
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
