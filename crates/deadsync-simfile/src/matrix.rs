use deadsync_chart::MatrixRatingInput;

pub fn matrix_rating_at_rate(
    base_rating: f64,
    profile: &[MatrixRatingInput],
    music_rate: f32,
) -> f64 {
    let base = if base_rating.is_finite() {
        base_rating.max(0.0)
    } else {
        0.0
    };
    let rate = if music_rate.is_finite() && music_rate > 0.0 {
        f64::from(music_rate)
    } else {
        1.0
    };
    if profile.is_empty() || (rate - 1.0).abs() < 0.0005 {
        return base;
    }

    let input_rating = |input: &MatrixRatingInput| {
        if !input.effective_bpm.is_finite() || input.effective_bpm <= 0.0 || input.measures == 0 {
            return 0.0;
        }
        rssp::matrix::get_difficulty(input.effective_bpm * rate, input.measures as f64)
    };
    let rating = match profile {
        [input] => input_rating(input),
        inputs => inputs
            .iter()
            .fold(0.0f64, |best, input| best.max(input_rating(input))),
    };
    if rating > 0.0 { rating } else { base }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_rating_matches_rssp_profile_evaluation() {
        let rssp_profile = [
            rssp::matrix::MatrixRatingInput {
                effective_bpm: 180.0,
                measures: 32,
            },
            rssp::matrix::MatrixRatingInput {
                effective_bpm: 300.0,
                measures: 12,
            },
        ];
        let profile: Vec<_> = rssp_profile
            .iter()
            .map(|input| MatrixRatingInput {
                effective_bpm: input.effective_bpm,
                measures: input.measures,
            })
            .collect();

        for rate in [0.8f32, 1.25, 1.5] {
            assert_eq!(
                matrix_rating_at_rate(12.0, &profile, rate),
                rssp::matrix::matrix_rating_at_rate(&rssp_profile, f64::from(rate))
            );
        }
    }

    #[test]
    fn base_rating_covers_one_x_and_missing_profiles() {
        let profile = [MatrixRatingInput {
            effective_bpm: 180.0,
            measures: 32,
        }];

        assert_eq!(matrix_rating_at_rate(12.34, &profile, 1.0), 12.34);
        assert_eq!(matrix_rating_at_rate(12.34, &[], 1.25), 12.34);
        assert_eq!(matrix_rating_at_rate(12.34, &profile, f32::NAN), 12.34);
    }
}
