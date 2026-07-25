#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LoadingAnimation {
    Pulse,
}

impl LoadingAnimation {
    pub fn from_str(s: &str) -> Self {
        match s {
            "pulse" => Self::Pulse,
            _ => Self::Pulse,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pulse => "pulse",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Pulse => "Pulse",
        }
    }

    pub fn next(&self) -> Self {
        Self::Pulse
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loading_animation_defaults_to_pulse() {
        assert_eq!(LoadingAnimation::from_str(""), LoadingAnimation::Pulse);
        assert_eq!(LoadingAnimation::from_str("wave"), LoadingAnimation::Pulse);
        assert_eq!(LoadingAnimation::from_str("skeleton"), LoadingAnimation::Pulse);
        assert_eq!(LoadingAnimation::from_str("bar"), LoadingAnimation::Pulse);
        assert_eq!(LoadingAnimation::from_str("pulse"), LoadingAnimation::Pulse);
        assert_eq!(LoadingAnimation::from_str("bounce"), LoadingAnimation::Pulse);
    }

    #[test]
    fn loading_animation_cycles_correctly() {
        assert_eq!(LoadingAnimation::Pulse.next(), LoadingAnimation::Pulse);
    }

    #[test]
    fn loading_animation_display_names_are_not_empty() {
        for variant in [LoadingAnimation::Pulse] {
            assert!(!variant.display_name().is_empty());
            assert!(!variant.as_str().is_empty());
        }
    }
}
