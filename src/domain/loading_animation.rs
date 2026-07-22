#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LoadingAnimation {
    Wave,
    SkeletonSweep,
    IndeterminateBar,
}

impl LoadingAnimation {
    pub fn from_str(s: &str) -> Self {
        match s {
            "skeleton" => Self::SkeletonSweep,
            "bar" => Self::IndeterminateBar,
            _ => Self::Wave,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Wave => "wave",
            Self::SkeletonSweep => "skeleton",
            Self::IndeterminateBar => "bar",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Wave => "Wave",
            Self::SkeletonSweep => "Skeleton Sweep",
            Self::IndeterminateBar => "Indeterminate Bar",
        }
    }

    pub fn next(&self) -> Self {
        match self {
            Self::Wave => Self::SkeletonSweep,
            Self::SkeletonSweep => Self::IndeterminateBar,
            Self::IndeterminateBar => Self::Wave,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loading_animation_defaults_to_wave() {
        assert_eq!(LoadingAnimation::from_str(""), LoadingAnimation::Wave);
        assert_eq!(LoadingAnimation::from_str("wave"), LoadingAnimation::Wave);
        assert_eq!(LoadingAnimation::from_str("skeleton"), LoadingAnimation::SkeletonSweep);
        assert_eq!(LoadingAnimation::from_str("bar"), LoadingAnimation::IndeterminateBar);
    }

    #[test]
    fn loading_animation_cycles_correctly() {
        assert_eq!(LoadingAnimation::Wave.next(), LoadingAnimation::SkeletonSweep);
        assert_eq!(LoadingAnimation::SkeletonSweep.next(), LoadingAnimation::IndeterminateBar);
        assert_eq!(LoadingAnimation::IndeterminateBar.next(), LoadingAnimation::Wave);
    }

    #[test]
    fn loading_animation_display_names_are_not_empty() {
        for variant in [LoadingAnimation::Wave, LoadingAnimation::SkeletonSweep, LoadingAnimation::IndeterminateBar] {
            assert!(!variant.display_name().is_empty());
            assert!(!variant.as_str().is_empty());
        }
    }
}
