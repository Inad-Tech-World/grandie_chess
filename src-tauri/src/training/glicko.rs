//! Glicko-2 rating updates (Glickman, "Example of the Glicko-2 system",
//! http://www.glicko.net/glicko/glicko2.pdf), used for the puzzle rating.

use serde::{Deserialize, Serialize};
use specta::Type;

const SCALE: f64 = 173.7178;
const TAU: f64 = 0.5;
const EPSILON: f64 = 0.000_001;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Rating {
    pub rating: f64,
    pub deviation: f64,
    pub volatility: f64,
}

impl Default for Rating {
    fn default() -> Self {
        Rating { rating: 1500.0, deviation: 350.0, volatility: 0.06 }
    }
}

/// One game: the opponent's rating and deviation, and our score (1, 0.5, 0).
#[derive(Debug, Clone, Copy)]
pub struct Outcome {
    pub opponent: f64,
    pub opponent_deviation: f64,
    pub score: f64,
}

fn g(phi: f64) -> f64 {
    1.0 / (1.0 + 3.0 * phi * phi / (std::f64::consts::PI * std::f64::consts::PI)).sqrt()
}

fn e(mu: f64, mu_j: f64, phi_j: f64) -> f64 {
    1.0 / (1.0 + (-g(phi_j) * (mu - mu_j)).exp())
}

/// Update a rating after a rating period with the given results.
pub fn update(r: Rating, results: &[Outcome]) -> Rating {
    let mu = (r.rating - 1500.0) / SCALE;
    let phi = r.deviation / SCALE;
    if results.is_empty() {
        let phi_star = (phi * phi + r.volatility * r.volatility).sqrt();
        return Rating { deviation: (phi_star * SCALE).min(350.0), ..r };
    }
    let mut v_inv = 0.0;
    let mut delta_sum = 0.0;
    for o in results {
        let mu_j = (o.opponent - 1500.0) / SCALE;
        let phi_j = o.opponent_deviation / SCALE;
        let e_j = e(mu, mu_j, phi_j);
        v_inv += g(phi_j).powi(2) * e_j * (1.0 - e_j);
        delta_sum += g(phi_j) * (o.score - e_j);
    }
    let v = 1.0 / v_inv;
    let delta = v * delta_sum;

    // New volatility (Illinois algorithm, step 5 of the paper).
    let a = (r.volatility * r.volatility).ln();
    let f = |x: f64| {
        let ex = x.exp();
        ex * (delta * delta - phi * phi - v - ex) / (2.0 * (phi * phi + v + ex).powi(2)) - (x - a) / (TAU * TAU)
    };
    let mut big_a = a;
    let mut big_b = if delta * delta > phi * phi + v {
        (delta * delta - phi * phi - v).ln()
    } else {
        let mut k = 1.0;
        while f(a - k * TAU) < 0.0 {
            k += 1.0;
        }
        a - k * TAU
    };
    let mut f_a = f(big_a);
    let mut f_b = f(big_b);
    while (big_b - big_a).abs() > EPSILON {
        let big_c = big_a + (big_a - big_b) * f_a / (f_b - f_a);
        let f_c = f(big_c);
        if f_c * f_b <= 0.0 {
            big_a = big_b;
            f_a = f_b;
        } else {
            f_a /= 2.0;
        }
        big_b = big_c;
        f_b = f_c;
    }
    let sigma = (big_a / 2.0).exp();

    let phi_star = (phi * phi + sigma * sigma).sqrt();
    let phi_new = 1.0 / (1.0 / (phi_star * phi_star) + 1.0 / v).sqrt();
    let mu_new = mu + phi_new * phi_new * delta_sum;
    Rating {
        rating: mu_new * SCALE + 1500.0,
        // Keep some uncertainty so the rating keeps moving with practice.
        deviation: (phi_new * SCALE).clamp(45.0, 350.0),
        volatility: sigma,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_glickmans_worked_example() {
        let player = Rating { rating: 1500.0, deviation: 200.0, volatility: 0.06 };
        let results = [
            Outcome { opponent: 1400.0, opponent_deviation: 30.0, score: 1.0 },
            Outcome { opponent: 1550.0, opponent_deviation: 100.0, score: 0.0 },
            Outcome { opponent: 1700.0, opponent_deviation: 300.0, score: 0.0 },
        ];
        let r = update(player, &results);
        assert!((r.rating - 1464.06).abs() < 0.05, "{}", r.rating);
        assert!((r.deviation - 151.52).abs() < 0.05, "{}", r.deviation);
        assert!((r.volatility - 0.05999).abs() < 0.0001, "{}", r.volatility);
    }

    #[test]
    fn wins_raise_and_losses_lower() {
        let start = Rating::default();
        let win = update(start, &[Outcome { opponent: 1500.0, opponent_deviation: 80.0, score: 1.0 }]);
        let loss = update(start, &[Outcome { opponent: 1500.0, opponent_deviation: 80.0, score: 0.0 }]);
        assert!(win.rating > 1500.0 && loss.rating < 1500.0);
        assert!(win.deviation < 350.0);
        assert!(update(start, &[]).deviation <= 350.0);
    }
}
