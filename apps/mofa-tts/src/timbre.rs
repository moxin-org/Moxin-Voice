//! Output timbre options for TTS requests.
//!
//! This module centralizes front-end option bounds and request encoding.

pub const TTSCFG_PREFIX: &str = "TTSCFG";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputSpeed {
    Slow,
    Normal,
    Fast,
}

impl Default for OutputSpeed {
    fn default() -> Self {
        Self::Normal
    }
}

impl OutputSpeed {
    pub fn factor(self) -> f32 {
        match self {
            Self::Slow => 0.85,
            Self::Normal => 1.0,
            Self::Fast => 1.15,
        }
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Self::Slow => "tts.timbre.speed_slow",
            Self::Normal => "tts.timbre.speed_normal",
            Self::Fast => "tts.timbre.speed_fast",
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Self::Slow => "slow",
            Self::Normal => "normal",
            Self::Fast => "fast",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputPitch {
    Low,
    Normal,
    High,
}

impl Default for OutputPitch {
    fn default() -> Self {
        Self::Normal
    }
}

impl OutputPitch {
    pub fn semitones(self) -> i32 {
        match self {
            Self::Low => -3,
            Self::Normal => 0,
            Self::High => 3,
        }
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Self::Low => "tts.timbre.pitch_low",
            Self::Normal => "tts.timbre.pitch_normal",
            Self::High => "tts.timbre.pitch_high",
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Normal => "normal",
            Self::High => "high",
        }
    }
}

pub fn build_prompt_with_timbre(
    base_prompt: &str,
    speed: OutputSpeed,
    pitch: OutputPitch,
) -> String {
    format!(
        "{TTSCFG_PREFIX}|{:.2}|{}|{}",
        speed.factor(),
        pitch.semitones(),
        base_prompt
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_normal() {
        assert_eq!(OutputSpeed::default(), OutputSpeed::Normal);
        assert_eq!(OutputPitch::default(), OutputPitch::Normal);
    }

    #[test]
    fn encodes_prompt_with_ttscfg_prefix() {
        let wrapped = build_prompt_with_timbre("VOICE:Doubao|hello", OutputSpeed::Fast, OutputPitch::High);
        assert!(wrapped.starts_with("TTSCFG|1.15|3|VOICE:Doubao|hello"));
    }

    #[test]
    fn speed_and_pitch_have_bounded_values() {
        assert!((OutputSpeed::Slow.factor() - 0.85).abs() < 0.001);
        assert!((OutputSpeed::Fast.factor() - 1.15).abs() < 0.001);
        assert_eq!(OutputPitch::Low.semitones(), -3);
        assert_eq!(OutputPitch::High.semitones(), 3);
    }
}
