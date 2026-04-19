/// POS class for connection cost calculation.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum PosClass {
    Noun,      // 名詞
    Verb,      // 動詞
    Adjective, // 形容詞
    Particle,  // 助詞
    Copula,    // 助動詞
    SentFinal, // 終助詞
    Unknown,   // BOS / OOV / unknown
}

/// Classify a POS string into PosClass.
pub fn classify(pos: &str) -> PosClass {
    match pos {
        "名詞" => PosClass::Noun,
        "動詞" => PosClass::Verb,
        "形容詞" => PosClass::Adjective,
        "助詞" => PosClass::Particle,
        "助動詞" => PosClass::Copula,
        "終助詞" => PosClass::SentFinal,
        _ => PosClass::Unknown,
    }
}

/// Returns additive penalty (≥0.0) for a prev→next POS bigram transition.
/// Multiply by cost_pos_alpha to get the score deduction.
pub fn connection_penalty(prev: PosClass, next: PosClass) -> f64 {
    use PosClass::*;
    match (prev, next) {
        // 自然な文節内遷移 — penalty 0
        (Noun, Particle) | (Noun, Copula) => 0.0,
        (Verb, Particle) | (Verb, Copula) | (Verb, SentFinal) => 0.0,
        (Adjective, Noun) | (Adjective, Copula) | (Adjective, SentFinal) => 0.0,
        (Particle, Noun) | (Particle, Verb) | (Particle, Adjective) => 0.0,
        (Copula, SentFinal) => 0.0,
        // 可能だがやや珍しい遷移
        (Noun, Verb) | (Noun, Adjective) => 1.0,
        (Noun, Noun) => 1.5,        // 複合名詞: allowed but adds cost
        (Adjective, Particle) | (Adjective, Verb) => 1.0,
        (Adjective, Adjective) => 1.5,
        (Copula, Particle) | (Copula, Noun) | (Copula, Verb) => 1.5,
        (Verb, Verb) => 2.0,
        // 不自然な遷移 — large penalty
        (Particle, Particle) => 5.0,
        (Particle, Copula) => 3.0,
        (SentFinal, _) => 8.0,      // sentence-final should not be followed by anything
        // BOS / OOV: no penalty
        (Unknown, _) => 0.0,
        // Other unexpected combinations
        _ => 2.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noun_particle_is_zero() {
        assert_eq!(connection_penalty(PosClass::Noun, PosClass::Particle), 0.0);
    }

    #[test]
    fn particle_particle_is_high() {
        assert!(connection_penalty(PosClass::Particle, PosClass::Particle) >= 4.0);
    }

    #[test]
    fn sent_final_followed_by_anything_is_very_high() {
        assert!(connection_penalty(PosClass::SentFinal, PosClass::Noun) >= 5.0);
    }

    #[test]
    fn unknown_prev_is_zero() {
        assert_eq!(connection_penalty(PosClass::Unknown, PosClass::Noun), 0.0);
    }

    #[test]
    fn classify_known_pos_strings() {
        assert_eq!(classify("名詞"), PosClass::Noun);
        assert_eq!(classify("助詞"), PosClass::Particle);
        assert_eq!(classify("助動詞"), PosClass::Copula);
        assert_eq!(classify("終助詞"), PosClass::SentFinal);
        assert_eq!(classify("動詞"), PosClass::Verb);
        assert_eq!(classify("形容詞"), PosClass::Adjective);
        assert_eq!(classify("unknown"), PosClass::Unknown);
    }
}
