#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LoadingAnimation;

impl LoadingAnimation {
    pub fn from_str(_s: &str) -> Self {
        Self
    }

    pub fn as_str(&self) -> &'static str {
        "pulse"
    }

    pub fn display_name(&self) -> &'static str {
        "Pulse"
    }

    pub fn next(&self) -> Self {
        Self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loading_animation_defaults_to_pulse() {
        assert_eq!(LoadingAnimation::from_str(""), LoadingAnimation);
        assert_eq!(LoadingAnimation::from_str("wave"), LoadingAnimation);
        assert_eq!(LoadingAnimation::from_str("skeleton"), LoadingAnimation);
        assert_eq!(LoadingAnimation::from_str("bar"), LoadingAnimation);
        assert_eq!(LoadingAnimation::from_str("pulse"), LoadingAnimation);
        assert_eq!(LoadingAnimation::from_str("bounce"), LoadingAnimation);
    }

    #[test]
    fn loading_animation_cycles_correctly() {
        assert_eq!(LoadingAnimation.next(), LoadingAnimation);
    }

    #[test]
    fn loading_animation_display_names_are_not_empty() {
        let anim = LoadingAnimation;
        assert!(!anim.display_name().is_empty());
        assert!(!anim.as_str().is_empty());
    }
}
