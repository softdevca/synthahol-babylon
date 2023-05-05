use std::convert::TryFrom;

use strum::IntoEnumIterator;
use strum_macros::EnumIter;
use uom::si::f64::Ratio;

use crate::Envelope;

#[derive(Debug)]
pub struct Chorus {
    pub enabled: bool,
    pub depth: f64,
    pub pre_delay: f64,
    pub ratio: f64,
    pub mix: f64,
}

impl Effect for Chorus {
    fn is_enabled(&self) -> bool {
        self.enabled
    }
}

#[derive(Debug)]
pub struct Delay {
    pub enabled: bool,
    pub ping_pong: bool,
    pub feedback: f64,
    pub filter: f64,
    pub sync: bool,
    pub time: f64,
    pub mix: f64,
}

impl Effect for Delay {
    fn is_enabled(&self) -> bool {
        self.enabled
    }
}

#[derive(Debug)]
pub struct Distortion {
    pub enabled: bool,

    /// 0.0 to 10.0
    pub gain: f64,
}

impl Effect for Distortion {
    fn is_enabled(&self) -> bool {
        self.enabled
    }
}

pub trait Effect {
    fn is_enabled(&self) -> bool {
        false
    }
}

#[derive(Debug)]
pub struct Equalizer {
    pub enabled: bool,
    pub high_gain: Ratio,
    pub low_gain: Ratio,
    pub mid_gain: Ratio,
}

impl Effect for Equalizer {
    fn is_enabled(&self) -> bool {
        self.enabled
    }
}

/// Kinds of effects. The discriminants of the items match the file format. This is the default
/// ordering of the effects.
#[derive(Copy, Clone, Debug, EnumIter, Eq, PartialEq)]
#[repr(u32)]
pub enum EffectType {
    Distortion,
    LoFi,
    Filter,
    Chorus,
    Equalizer,
    Delay,
    Reverb,
}

impl TryFrom<u32> for EffectType {
    type Error = String;

    fn try_from(effect_type_id: u32) -> Result<Self, Self::Error> {
        EffectType::iter()
            .find(|id| *id as u32 == effect_type_id)
            .ok_or(format!("Unknown effect type ID {}", effect_type_id))
    }
}

/// The discriminants of the items match the file format.
#[derive(Copy, Clone, Debug, EnumIter, Eq, PartialEq)]
#[repr(u32)]
pub enum FilterMode {
    LowPass,
    BandPass,
    HighPass,
    Notch,
    Peak,
}

impl FilterMode {
    pub(crate) fn from_or(mode_id: u32, default: FilterMode) -> FilterMode {
        FilterMode::iter()
            .find(|id| *id as u32 == mode_id)
            .unwrap_or(default)
    }
}

/// The discriminants of the items match the file format.
#[derive(Copy, Clone, Debug, EnumIter, Eq, PartialEq)]
#[repr(u32)]
pub enum FilterEffectMode {
    Off,
    Saturation,
    Overdrive,
    Distortion,
    BitRateReduction,
    SampleRateReduction,
}

impl FilterEffectMode {
    pub(crate) fn from_or(mode_id: u32, default: FilterEffectMode) -> FilterEffectMode {
        FilterEffectMode::iter()
            .find(|id| *id as u32 == mode_id)
            .unwrap_or(default)
    }
}

#[derive(Debug)]
pub struct Filter {
    pub enabled: bool,
    pub mode: FilterMode,
    pub resonance: f64,
    pub cutoff_frequency: f64,
    pub key_tracking: f64,
    pub envelope: Envelope,

    /// How much the envelope affects the cutoff frequency
    pub envelope_amount: f64,

    /// How the effect is processed.
    pub effect_mode: FilterEffectMode,
    pub effect_enabled: bool,
    pub effect_amount: f64,
}

impl Effect for Filter {
    fn is_enabled(&self) -> bool {
        self.enabled
    }
}

#[derive(Debug)]
pub struct LoFi {
    pub enabled: bool,
    pub bitrate: f64,

    // 0 to 10.0 in Babylon interface
    pub sample_rate: f64,

    // 0 to 10.0 in Babylon interface
    pub mix: f64,
}

impl Effect for LoFi {
    fn is_enabled(&self) -> bool {
        self.enabled
    }
}

#[derive(Debug)]
pub struct Reverb {
    pub enabled: bool,
    pub dampen: f64,
    pub filter: f64,
    pub room: f64,
    pub width: f64,
    pub mix: f64,
}

impl Effect for Reverb {
    fn is_enabled(&self) -> bool {
        self.enabled
    }
}

#[cfg(test)]
mod test {
    use std::io::Result;
    use std::path::Path;

    use approx::assert_relative_eq;
    use strum::IntoEnumIterator;
    use uom::si::ratio::percent;

    use crate::{EffectType, FilterMode, Preset};

    fn read_preset(filename: &str) -> Result<Preset> {
        let path = &Path::new("tests").join("effects").join(&filename);
        Preset::read_file(path)
    }

    #[test]
    fn delay() {
        let preset = read_preset("delay-ping_pong_off-1.0.2.bab").unwrap();
        assert!(!preset.delay.ping_pong);

        let preset = read_preset("delay-ping_pong_on-1.0.2.bab").unwrap();
        assert!(preset.delay.ping_pong);

        let preset = read_preset("delay-time1t-hp100-ping_pong-1.0.3.bab").unwrap();
        assert!(preset.delay.ping_pong);
        assert!(preset.delay.sync);
        assert_eq!(preset.delay.time, 1.0);
        assert_relative_eq!(preset.delay.filter, 0.667, epsilon = 0.00001);

        let preset = read_preset("delay-time504-syncoff-1.0.3.bab").unwrap();
        assert!(!preset.delay.sync);
        assert_relative_eq!(preset.delay.time, 0.504, epsilon = 0.00001);

        let preset = read_preset("delay-timehalf-lp200-1.0.3.bab").unwrap();
        assert_relative_eq!(preset.delay.time, 0.257, epsilon = 0.00001);
        assert_relative_eq!(preset.delay.filter, 0.333, epsilon = 0.00001);

        let preset = read_preset("delay-timesixteenth-bp3000-1.0.3.bab").unwrap();
        assert_relative_eq!(preset.delay.time, 0.410, epsilon = 0.00001);
        assert_relative_eq!(preset.delay.filter, 0.708, epsilon = 0.00001);
    }

    #[test]
    fn distortion() {
        let preset = read_preset("distortion-gain5-1.0.3.bab").unwrap();
        assert!(preset.distortion.enabled);
        assert_eq!(preset.distortion.gain, 0.5);
    }

    #[test]
    fn effect_order() {
        let preset = read_preset("effect-order-reversed-1.0.2.bab").unwrap();
        let expected_effect_order: Vec<EffectType> = EffectType::iter().rev().collect();
        assert_eq!(&preset.effect_order, &expected_effect_order);
        assert_eq!(preset.effect_position(EffectType::Equalizer).unwrap(), 2);
    }

    #[test]
    fn equalizer() {
        let preset = read_preset("equalizer-l-10-m5-h-10-1.0.3.bab").unwrap();
        assert!(preset.equalizer.enabled);
        assert_eq!(preset.equalizer.low_gain.get::<percent>(), 0.5);
        assert_eq!(preset.equalizer.mid_gain.get::<percent>(), 0.5);
        assert_eq!(preset.equalizer.high_gain.get::<percent>(), 0.5);
    }

    #[test]
    fn filter() {
        let preset = read_preset("filter-bandpass-1.0.2.bab").unwrap();
        assert_eq!(preset.filter.mode, FilterMode::BandPass);
        assert_eq!(preset.filter.cutoff_frequency, 100.0);

        let preset = read_preset("filter-highpass-1.0.2.bab").unwrap();
        assert_eq!(preset.filter.mode, FilterMode::HighPass);

        let preset = read_preset("filter-notch-1.0.2.bab").unwrap();
        assert_eq!(preset.filter.mode, FilterMode::Notch);

        let preset = read_preset("filter-peak-1.0.2.bab").unwrap();
        assert_eq!(preset.filter.mode, FilterMode::Peak);
    }

    #[test]
    fn reverb() {
        let preset = read_preset("reverb-r100-w0-d50-m34-hp400-1.0.3.bab").unwrap();
        assert!(preset.reverb.enabled);
        assert_relative_eq!(preset.reverb.room, 1.0, epsilon = 0.0001);
        assert_relative_eq!(preset.reverb.width, 0.0, epsilon = 0.0001);
        assert_relative_eq!(preset.reverb.dampen, 0.50, epsilon = 0.0001);
        assert_relative_eq!(preset.reverb.mix, 0.34, epsilon = 0.0001);
        assert_relative_eq!(preset.reverb.filter, 0.583, epsilon = 0.0001);
    }
}
