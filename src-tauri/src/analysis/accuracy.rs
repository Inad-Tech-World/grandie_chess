//! Win percentages and accuracy, following the formulas Lichess publishes
//! (https://lichess.org/page/accuracy), with lc0's WDL output used for the
//! win percentage when available.

use crate::engine::uci::{Score, Wdl};

/// Win percentage (0..100) from centipawns, for engines without WDL output.
pub fn win_from_cp(cp: i32) -> f64 {
    let cp = cp.clamp(-1000, 1000) as f64;
    50.0 + 50.0 * (2.0 / (1.0 + (-0.003_682_08 * cp).exp()) - 1.0)
}

/// Win percentage (0..100) for the side the score/WDL refers to.
pub fn win_percent(score: Option<Score>, wdl: Option<Wdl>) -> Option<f64> {
    if let Some(w) = wdl {
        return Some(w.expected_score() * 100.0);
    }
    match score? {
        Score::Cp(cp) => Some(win_from_cp(cp)),
        Score::Mate(m) if m > 0 => Some(100.0),
        Score::Mate(_) => Some(0.0),
    }
}

/// Accuracy of one move (0..100) from the mover's win% before and after.
pub fn move_accuracy(win_before: f64, win_after: f64) -> f64 {
    let drop = (win_before - win_after).max(0.0);
    (103.1668 * (-0.04354 * drop).exp() - 3.1669 + 1.0).clamp(0.0, 100.0)
}

fn std_dev(xs: &[f64]) -> f64 {
    if xs.len() < 2 {
        return 0.0;
    }
    let mean = xs.iter().sum::<f64>() / xs.len() as f64;
    (xs.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / xs.len() as f64).sqrt()
}

/// Game accuracy for one side, as on Lichess: the average of a
/// volatility-weighted mean and the harmonic mean of the move accuracies.
///
/// `white_wins` is White's win% for every position (start included);
/// `accuracies[i]` belongs to the move from position i to i+1.
pub fn game_accuracy(white_wins: &[f64], accuracies: &[f64], white: bool) -> Option<f64> {
    if accuracies.is_empty() || white_wins.len() != accuracies.len() + 1 {
        return None;
    }
    let n = white_wins.len();
    let window = (n / 10).clamp(2, 8);
    let mut weights = Vec::with_capacity(accuracies.len());
    for i in 0..accuracies.len() {
        let start = (i + 1).saturating_sub(window);
        let end = (start + window).min(n);
        weights.push(std_dev(&white_wins[start..end]).clamp(0.5, 12.0));
    }
    let (mut wsum, mut wtot, mut harm, mut count) = (0.0, 0.0, 0.0, 0.0);
    for (i, acc) in accuracies.iter().enumerate() {
        let is_white_move = i % 2 == 0;
        if is_white_move != white {
            continue;
        }
        wsum += acc * weights[i];
        wtot += weights[i];
        harm += 1.0 / acc.max(1.0);
        count += 1.0;
    }
    if count == 0.0 {
        return None;
    }
    let weighted = wsum / wtot;
    let harmonic = count / harm;
    Some(((weighted + harmonic) / 2.0).clamp(0.0, 100.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn win_percent_from_cp_and_wdl() {
        assert!((win_from_cp(0) - 50.0).abs() < 1e-9);
        assert!(win_from_cp(300) > 74.0 && win_from_cp(300) < 76.0);
        assert!((win_from_cp(-300) - (100.0 - win_from_cp(300))).abs() < 1e-9);
        assert_eq!(win_percent(Some(Score::Mate(3)), None), Some(100.0));
        assert_eq!(win_percent(Some(Score::Mate(-1)), None), Some(0.0));
        let wdl = Wdl { w: 500, d: 400, l: 100 };
        assert_eq!(win_percent(Some(Score::Cp(999)), Some(wdl)), Some(70.0));
    }

    #[test]
    fn move_accuracy_matches_lichess_curve() {
        assert!((move_accuracy(50.0, 50.0) - 100.0).abs() < 0.01);
        assert!(move_accuracy(50.0, 55.0) >= 99.9, "improving is never penalised");
        let small = move_accuracy(60.0, 55.0);
        let big = move_accuracy(60.0, 30.0);
        assert!(small > 80.0 && small < 85.0, "{small}");
        assert!(big < 30.0, "{big}");
    }

    #[test]
    fn game_accuracy_rewards_clean_play() {
        let wins = vec![50.0, 52.0, 51.0, 53.0, 52.0, 54.0, 53.0];
        let perfect = vec![100.0; 6];
        assert!(game_accuracy(&wins, &perfect, true).unwrap() > 99.0);
        let mixed = vec![100.0, 40.0, 100.0, 40.0, 100.0, 40.0];
        let white = game_accuracy(&wins, &mixed, true).unwrap();
        let black = game_accuracy(&wins, &mixed, false).unwrap();
        assert!(white > 95.0 && black < 45.0, "{white} {black}");
        assert_eq!(game_accuracy(&wins, &[100.0], true), None);
    }
}
