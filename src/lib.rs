//! [Babylon](https://www.waproduction.com/plugins/view/babylon) is a virtual
//! analog synth by W. A. Production.
//!
//! # Reading a Preset
//!
//! ```rust
//! use synthahol_babylon::Preset;
//!
//! let path = std::path::Path::new("tests") .join("init-1.0.2.bab");
//! let preset = Preset::read_file(&path).unwrap();
//! ```
//!
//! # Developer
//!
//! Babylon was written by Rahman Fotouhi at
//! [http://rfmusic.net](http://rfmusic.net). The Wayback Machine has a
//! [version of his site](https://web.archive.org/web/20200806002200/http://rfmusic.net/en/)
//! before it linked to W. A. Production.
//!
//! # Implementation
//!
//! Babylon 1.0.3 was written using JUCE 6.0.8.
//!
//! Version 1.0.2 has build number 15.

use std::convert::TryFrom;
use std::fmt::{Debug, Display, Formatter};
use std::fs::File;
use std::io::{BufReader, Error, ErrorKind};
use std::path::Path;
use std::str::FromStr;

use log::warn;
use serde::{Deserialize, Serialize};
use serde_xml_rs::de::from_reader;
use strum::IntoEnumIterator;
use strum_macros::{AsRefStr, EnumIter};
use uom::num::Zero;
use uom::si::f64::{Ratio, Time};
use uom::si::ratio::percent;
use uom::si::time::{millisecond, second};

pub use effect::*;

mod effect;

const MODULATION_MATRIX_SIZE: usize = 8;

/// The standard Preset Info text if the user does not change it.  It is treated as blank.
const PRESET_INFO_DEFAULT: &str = "Preset Info";

/// ADSR-style envelope.
#[derive(Clone, Debug, PartialEq)]
pub struct Envelope {
    pub attack: Time,

    #[doc(alias = "attack_slope")]
    pub attack_curve: f64,

    pub decay: Time,

    #[doc(alias = "decay_slope")]
    pub decay_falloff: f64,

    /// A percentage, not milliseconds
    pub sustain: Ratio,

    pub release: Time,

    #[doc(alias = "release_slope")]
    pub release_falloff: f64,
}

#[derive(Debug)]
pub enum EnvelopeCurve {
    Linear,
    Exponential1,
    Exponential2,
    Exponential3,
    Exponential4,
    Logarithmic1,
    Logarithmic2,
    Pluck1,
    Pluck2,
    Pluck3,

    /// Exp to Log
    DoubleCurve1,

    /// Log to Exp
    DoubleCurve2,
}

impl EnvelopeCurve {
    pub fn value(self) -> f64 {
        use EnvelopeCurve::*;
        match self {
            Linear => 0.000,
            Exponential1 => 0.070,
            Exponential2 => 0.133,
            Exponential3 => 0.200,
            Exponential4 => 0.267,
            Logarithmic1 => 0.333,
            Logarithmic2 => 0.400,
            Pluck1 => 0.467,
            Pluck2 => 0.533,
            Pluck3 => 0.600,
            DoubleCurve1 => 0.667,
            DoubleCurve2 => 0.733,
        }
    }
}

impl Display for EnvelopeCurve {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        use EnvelopeCurve::*;
        write!(
            f,
            "{}",
            match self {
                Linear => "Linear",
                Exponential1 => "Exponential 1",
                Exponential2 => "Exponential 2",
                Exponential3 => "Exponential 3",
                Exponential4 => "Exponential 4",
                Logarithmic1 => "Logarithmic 1",
                Logarithmic2 => "Logarithmic 2",
                Pluck1 => "Pluck 1",
                Pluck2 => "Pluck 2",
                Pluck3 => "Pluck 3",
                DoubleCurve1 => "Double Curve 1: Exp > Log",
                DoubleCurve2 => "Double Curve 2: Log > Exp",
            }
        )
    }
}

#[derive(Debug)]
pub struct Lfo {
    pub enabled: bool,
    pub waveform: Waveform,
    pub sync: bool,
    pub invert: bool,
    pub reverse: bool,
    pub mono: bool,
    pub free_run: bool,
    pub frequency: f64,
    pub phase: f64,
}

#[derive(Debug)]
pub struct MatrixItem {
    pub source: u32,
    pub target: u32,
    pub amount: f64,
}

/// White noise generator.
#[derive(Debug)]
pub struct Noise {
    pub enabled: bool,
    pub width: f64,
    pub pan: f64,
    pub volume: f64,
}

impl Effect for Noise {}

/// The third oscillator doesn't have all the capabilities of the first two
/// oscillators because the first two route to the third.
#[derive(Debug)]
pub struct Oscillator {
    pub enabled: bool,
    pub waveform: Waveform,
    pub invert: bool,
    pub pan: f64,
    pub phase: f64,

    pub pitch: f64,
    pub fine_tuning: i32,
    pub semitone_tuning: i32,
    pub octave_tuning: i32,

    pub reverse: bool,
    pub free_run: bool,
    pub sync_all: bool,
    pub volume: f64,
    pub unison: Unison,

    /// Amplitude modulation
    pub am_enabled: bool,
    pub am_amount: f64,

    /// Frequency modulation
    pub fm_enabled: bool,
    pub fm_amount: f64,

    /// Ring modulations
    pub rm_enabled: bool,
    pub rm_amount: f64,
}

/// The discriminants of the items match the file format.
#[derive(Copy, Clone, Debug, EnumIter, Eq, PartialEq)]
#[repr(u32)]
pub enum MidiPlayMode {
    Normal,

    /// Mute off-key note
    Cheat1,

    /// Replace off-key notes
    Cheat2,
}

impl MidiPlayMode {
    fn from_or(mode_id: u32, default: MidiPlayMode) -> MidiPlayMode {
        MidiPlayMode::iter()
            .find(|id| *id as u32 == mode_id)
            .unwrap_or(default)
    }
}

#[derive(Debug)]
pub struct ModulatorEnvelope {
    pub enabled: bool,
    pub envelope: Envelope,
    pub curve: f64,
}

/// The discriminants of the items match the file format.
#[derive(Copy, Clone, Debug, EnumIter, Eq, PartialEq)]
#[repr(u32)]
pub enum PortamentoMode {
    Poly,
    Legato,
    LegatoNoRetrigger,
    Porta,
    PortaPoly,
}

impl PortamentoMode {
    fn from_or(mode_id: u32, default: PortamentoMode) -> PortamentoMode {
        PortamentoMode::iter()
            .find(|id| *id as u32 == mode_id)
            .unwrap_or(default)
    }
}

#[derive(Debug)]
pub struct Tuning {
    pub transpose: f64,
    pub root_key: u32,
    pub scale: u32,

    /// Octave of values starting at A natural.
    pub tunings: [f64; 12],
}

#[derive(Debug)]
pub struct Vibrato {
    pub enabled: bool,
    pub attack: f64,
    pub delay: f64,
    pub frequency: f64,
}

#[derive(Debug)]
pub struct Unison {
    /// The first voice is the original signal.
    pub voices: u32,
    pub detune: f64,
    pub spread: f64,
    pub mix: f64,
}

/// The discriminant of the items match the file format.
#[derive(AsRefStr, Copy, Clone, Debug, EnumIter, Eq, PartialEq)]
#[repr(u32)]
pub enum Waveform {
    Sine,
    SineRoot1_5,
    SineRoot2,
    SineRoot3,
    SineRoot4,
    SinePower1_5,
    SinePower2,
    SinePower3,
    SinePower4,
    SineAm1,
    SineAm2,
    SineAm3,
    SineAm4,
    SineAm5,
    SineFmA1,
    SineFmA2,
    SineFmA3,
    SineFmA4,
    SineFmA5,
    SineFmA6,
    SineFmB1,
    SineFmB2,
    SineFmB3,
    SineFmB4,
    SineFmB5,
    SineFmC1,
    SineFmC2,
    SineFmC3,
    SineFmC4,
    SineFmC5,
    SineFmC6,
    SineFmC7,
    SineFmC8,
    SineFmD1,
    SineFmD2,
    SineFmD3,
    SineFmD4,
    SineFmD5,
    SineFmD6,
    SineFmD7,
    SineFmD8,
    SineFmD9,
    SineFmD10,
    SineFmD11,
    SineFmD12,
    SineFmD13,
    SineFmD14,
    SineFmD15,
    SineFmKick1,
    SineFmKick2,
    SineFmKick3,
    SineFmKick4,
    SineFmKick5,
    SineFmKick6,
    SineFmKick7,
    SineFmKick8,
    SineFmKick9,
    SineFmKick10,
    SineFmKick11,
    SineFmKick12,

    Triangle,
    TriangleRoot2,
    TriangleRoot3,
    TriangleRoot4,
    TriangleRoot5,

    Saw,
    SawPower1,
    SawPower2,
    SawSine1,
    SawSine2,
    SawSine3,
    Saw2x,

    Square,
    SquareSmooth1,
    SquareSmooth2,
    SquareHalfRoot,
    SquareHalfRootPower,
    SquarePower,
    SquareDoublePower1,
    SquareDoublePower2,
    SquareAttackPower,
    SquareTristate1,
    SquareTristate2,
    SquareTristate3,
    SquareTristate4,
    SquareTristate5,
    SquareTristate6,
    SquareFm1,
    SquareFm2,
    SquareFm3,
    SquareFm4,
    SquareFm5,
    SquareFm6,
    SquareFm7,
    SquareFm8,

    Pulse1,
    Pulse2,
    Pulse3,
    Pulse4,
    PulseSquare,
    PulseSquareSmooth,
    PulseSmooth1,
    PulseSmooth2,

    Voice1,
    Voice2,
    Voice3,
    Voice4,
    Voice5,
    Voice6,
    Voice7,
    Voice8,
    Voice9,
    Voice10,
    Voice11,
    Voice12,
    Voice13,
    Voice14,
    Voice15,
    Voice16,
    Voice17,
    Voice18,
    Voice19,
    Voice20,
    Voice21,
    Voice22,
    Voice23,
    Voice24,
    Voice25,
    Voice26,
    Voice27,
    Voice28,
    Voice29,
    Voice30,

    FormantA1,
    FormantA2,
    FormantA3,
    FormantA4,
    FormantA5,
    FormantA6,
    FormantA7,
    FormantA8,
    FormantB1,
    FormantB2,
    FormantB3,
    FormantB4,
    FormantB5,
    FormantB6,
    FormantB7,
    FormantB8,

    SyntheticVoice1,
    SyntheticVoice2,
    SyntheticVoice3,
    SyntheticVoice4,
    SyntheticVoice5,
    SyntheticVoice6,
    SyntheticVoice7,
    SyntheticVoice8,
    SyntheticVoice9,
    SyntheticVoice10,
    SyntheticVoice11,
    SyntheticVoice12,
    SyntheticVoice13,
    SyntheticVoice14,
    SyntheticVoice15,
    SyntheticVoice16,
    SyntheticVoice17,
    SyntheticVoice18,
    SyntheticVoice19,
    SyntheticVoice20,
    SyntheticVoice21,
    SyntheticVoice22,
    SyntheticVoice23,
    SyntheticVoice24,
    SyntheticVoice25,
    SyntheticVoice26,
    SyntheticVoice27,
    SyntheticVoice28,
    SyntheticVoice29,

    Organ1,
    Organ2,
    Organ3,
    Organ4,
    Organ5,
    Organ6,
    Organ7,
    Organ8,
    Organ9,
    Organ10,
    Organ11,
    Organ12,
    Organ13,
    Organ14,
    Organ15,
    Organ16,
    Organ17,
    Organ18,
    Organ19,
    Organ20,
    Organ21,
    Organ22,
    Organ23,
    EPiano1,
    EPiano2,
    EPiano3,
    EPiano4,
    Key1,
    Key2,
    Key3,
    DistGuitar1,
    DistGuitar2,
    Rhode,
    Brass1,
    Brass2,
    Chip1,
    Chip2,
    Chip3,
    Chip4,
    Chip5,
    Chip6,
    Chip7,

    Gritty1,
    Gritty2,
    Gritty3,
    Gritty4,
    Gritty5,
    Gritty6,
    Dirty1A,
    Dirty1B,
    Dirty1C,
    Dirty2A,
    Dirty2B,
    Dirty2C,
    Dirty3A,
    Dirty3B,
    Dirty3C,
    Dirty4A,
    Dirty4B,
    Dirty4C,
    Dirty5A,
    Dirty5B,
    Dirty5C,
    Dirty6A,
    Dirty6B,
    Dirty6C,
    Dirty7A,
    Dirty7B,
    Dirty7C,
    Dirty8A,
    Dirty8B,
    Dirty8C,

    Gate1,
    Gate2,
    Gate3,
    Gate4,
    Duck1,
    Duck2,
    Duck3,
}

impl Waveform {
    fn from_or(waveform_id: u32, default: Waveform) -> Waveform {
        Waveform::iter()
            .find(|id| *id as u32 == waveform_id)
            .unwrap_or(default)
    }
}

impl Display for Waveform {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        use Waveform::*;
        let s = match self {
            Sine => "Sine",
            SineRoot1_5 => "Sine Root 1.5",
            SineRoot2 => "Sine Root 2",
            SineRoot3 => "Sine Root 3",
            SineRoot4 => "Sine Root 4",
            SinePower1_5 => "Sine Power 1.5",
            SinePower2 => "Sine Power 2",
            SinePower3 => "Sine Power 3",
            SinePower4 => "Sine Power 4",
            SineAm1 => "Sine AM 1",
            SineAm2 => "Sine AM 2",
            SineAm3 => "Sine AM 3",
            SineAm4 => "Sine AM 4",
            SineAm5 => "Sine AM 5",
            SineFmA1 => "Sine FM A 1",
            SineFmA2 => "Sine FM A 2",
            SineFmA3 => "Sine FM A 3",
            SineFmA4 => "Sine FM A 4",
            SineFmA5 => "Sine FM A 5",
            SineFmA6 => "Sine FM A 6",
            SineFmB1 => "Sine FM B 1",
            SineFmB2 => "Sine FM B 2",
            SineFmB3 => "Sine FM B 3",
            SineFmB4 => "Sine FM B 4",
            SineFmB5 => "Sine FM B 5",
            SineFmC1 => "Sine FM C 1",
            SineFmC2 => "Sine FM C 2",
            SineFmC3 => "Sine FM C 3",
            SineFmC4 => "Sine FM C 4",
            SineFmC5 => "Sine FM C 5",
            SineFmC6 => "Sine FM C 6",
            SineFmC7 => "Sine FM C 7",
            SineFmC8 => "Sine FM C 8",
            SineFmD1 => "Sine FM D 1",
            SineFmD2 => "Sine FM D 2",
            SineFmD3 => "Sine FM D 3",
            SineFmD4 => "Sine FM D 4",
            SineFmD5 => "Sine FM D 5",
            SineFmD6 => "Sine FM D 6",
            SineFmD7 => "Sine FM D 7",
            SineFmD8 => "Sine FM D 8",
            SineFmD9 => "Sine FM D 9",
            SineFmD10 => "Sine FM D 10",
            SineFmD11 => "Sine FM D 11",
            SineFmD12 => "Sine FM D 12",
            SineFmD13 => "Sine FM D 13",
            SineFmD14 => "Sine FM D 14",
            SineFmD15 => "Sine FM D 15",
            SineFmKick1 => "Sine FM Kick 1",
            SineFmKick2 => "Sine FM Kick 2",
            SineFmKick3 => "Sine FM Kick 3",
            SineFmKick4 => "Sine FM Kick 4",
            SineFmKick5 => "Sine FM Kick 5",
            SineFmKick6 => "Sine FM Kick 6",
            SineFmKick7 => "Sine FM Kick 7",
            SineFmKick8 => "Sine FM Kick 8",
            SineFmKick9 => "Sine FM Kick 9",
            SineFmKick10 => "Sine FM Kick 10",
            SineFmKick11 => "Sine FM Kick 11",
            SineFmKick12 => "Sine FM Kick 12",
            Triangle => "Triangle",
            TriangleRoot2 => "Triangle Root 2",
            TriangleRoot3 => "Triangle Root 3",
            TriangleRoot4 => "Triangle Root 4",
            TriangleRoot5 => "Triangle Root 5",
            Saw => "Saw",
            SawPower1 => "Saw Power 1",
            SawPower2 => "Saw Power 2",
            SawSine1 => "Saw Sine 1",
            SawSine2 => "Saw Sine 2",
            SawSine3 => "Saw Sine 3",
            Saw2x => "Saw 2x",
            Square => "Square",
            SquareSmooth1 => "Square Smooth 1",
            SquareSmooth2 => "Square Smooth 2",
            SquareHalfRoot => "Square Half Root",
            SquareHalfRootPower => "Square Half Root Power",
            SquarePower => "Square Power",
            SquareDoublePower1 => "Square Double Power 1",
            SquareDoublePower2 => "Square Double Power 2",
            SquareAttackPower => "Square Attack Power",
            SquareTristate1 => "Square Tristate 1",
            SquareTristate2 => "Square Tristate 2",
            SquareTristate3 => "Square Tristate 3",
            SquareTristate4 => "Square Tristate 4",
            SquareTristate5 => "Square Tristate 5",
            SquareTristate6 => "Square Tristate 6",
            SquareFm1 => "Square FM 1",
            SquareFm2 => "Square FM 2",
            SquareFm3 => "Square FM 3",
            SquareFm4 => "Square FM 4",
            SquareFm5 => "Square FM 5",
            SquareFm6 => "Square FM 6",
            SquareFm7 => "Square FM 7",
            SquareFm8 => "Square FM 8",
            Pulse1 => "Pulse 1",
            Pulse2 => "Pulse 2",
            Pulse3 => "Pulse 3",
            Pulse4 => "Pulse 4",
            PulseSquare => "Pulse Square",
            PulseSquareSmooth => "Pulse Square Smooth",
            PulseSmooth1 => "Pulse Smooth 1",
            PulseSmooth2 => "Pulse Smooth 2",
            Voice1 => "Voice 1",
            Voice2 => "Voice 2",
            Voice3 => "Voice 3",
            Voice4 => "Voice 4",
            Voice5 => "Voice 5",
            Voice6 => "Voice 6",
            Voice7 => "Voice 7",
            Voice8 => "Voice 8",
            Voice9 => "Voice 9",
            Voice10 => "Voice 10",
            Voice11 => "Voice 11",
            Voice12 => "Voice 12",
            Voice13 => "Voice 13",
            Voice14 => "Voice 14",
            Voice15 => "Voice 15",
            Voice16 => "Voice 16",
            Voice17 => "Voice 17",
            Voice18 => "Voice 18",
            Voice19 => "Voice 19",
            Voice20 => "Voice 20",
            Voice21 => "Voice 21",
            Voice22 => "Voice 22",
            Voice23 => "Voice 23",
            Voice24 => "Voice 24",
            Voice25 => "Voice 25",
            Voice26 => "Voice 26",
            Voice27 => "Voice 27",
            Voice28 => "Voice 28",
            Voice29 => "Voice 29",
            Voice30 => "Voice 30",
            FormantA1 => "Formant A 1",
            FormantA2 => "Formant A 2",
            FormantA3 => "Formant A 3",
            FormantA4 => "Formant A 4",
            FormantA5 => "Formant A 5",
            FormantA6 => "Formant A 6",
            FormantA7 => "Formant A 7",
            FormantA8 => "Formant A 8",
            FormantB1 => "Formant B 1",
            FormantB2 => "Formant B 2",
            FormantB3 => "Formant B 3",
            FormantB4 => "Formant B 4",
            FormantB5 => "Formant B 5",
            FormantB6 => "Formant B 6",
            FormantB7 => "Formant B 7",
            FormantB8 => "Formant B 8",
            SyntheticVoice1 => "Synthetic Voice 1",
            SyntheticVoice2 => "Synthetic Voice 2",
            SyntheticVoice3 => "Synthetic Voice 3",
            SyntheticVoice4 => "Synthetic Voice 4",
            SyntheticVoice5 => "Synthetic Voice 5",
            SyntheticVoice6 => "Synthetic Voice 6",
            SyntheticVoice7 => "Synthetic Voice 7",
            SyntheticVoice8 => "Synthetic Voice 8",
            SyntheticVoice9 => "Synthetic Voice 9",
            SyntheticVoice10 => "Synthetic Voice 10",
            SyntheticVoice11 => "Synthetic Voice 11",
            SyntheticVoice12 => "Synthetic Voice 12",
            SyntheticVoice13 => "Synthetic Voice 13",
            SyntheticVoice14 => "Synthetic Voice 14",
            SyntheticVoice15 => "Synthetic Voice 15",
            SyntheticVoice16 => "Synthetic Voice 16",
            SyntheticVoice17 => "Synthetic Voice 17",
            SyntheticVoice18 => "Synthetic Voice 18",
            SyntheticVoice19 => "Synthetic Voice 19",
            SyntheticVoice20 => "Synthetic Voice 20",
            SyntheticVoice21 => "Synthetic Voice 21",
            SyntheticVoice22 => "Synthetic Voice 22",
            SyntheticVoice23 => "Synthetic Voice 23",
            SyntheticVoice24 => "Synthetic Voice 24",
            SyntheticVoice25 => "Synthetic Voice 25",
            SyntheticVoice26 => "Synthetic Voice 26",
            SyntheticVoice27 => "Synthetic Voice 27",
            SyntheticVoice28 => "Synthetic Voice 28",
            SyntheticVoice29 => "Synthetic Voice 39",
            Organ1 => "Organ 1",
            Organ2 => "Organ 2",
            Organ3 => "Organ 3",
            Organ4 => "Organ 4",
            Organ5 => "Organ 5",
            Organ6 => "Organ 6",
            Organ7 => "Organ 7",
            Organ8 => "Organ 8",
            Organ9 => "Organ 9",
            Organ10 => "Organ 10",
            Organ11 => "Organ 11",
            Organ12 => "Organ 12",
            Organ13 => "Organ 13",
            Organ14 => "Organ 14",
            Organ15 => "Organ 15",
            Organ16 => "Organ 16",
            Organ17 => "Organ 17",
            Organ18 => "Organ 18",
            Organ19 => "Organ 19",
            Organ20 => "Organ 20",
            Organ21 => "Organ 21",
            Organ22 => "Organ 22",
            Organ23 => "Organ 23",
            EPiano1 => "E Piano 1",
            EPiano2 => "E Piano 2",
            EPiano3 => "E Piano 3",
            EPiano4 => "E Piano 4",
            Key1 => "Key 1",
            Key2 => "Key 2",
            Key3 => "Key 3",
            DistGuitar1 => "Dist Guitar 1",
            DistGuitar2 => "Dist Guitar 2",
            Rhode => "Rhode",
            Brass1 => "Brass 1",
            Brass2 => "Brass 2",
            Chip1 => "Chip 1",
            Chip2 => "Chip 2",
            Chip3 => "Chip 3",
            Chip4 => "Chip 4",
            Chip5 => "Chip 5",
            Chip6 => "Chip 6",
            Chip7 => "Chip 7",
            Gritty1 => "Gritty 1",
            Gritty2 => "Gritty 2",
            Gritty3 => "Gritty 3",
            Gritty4 => "Gritty 4",
            Gritty5 => "Gritty 5",
            Gritty6 => "Gritty 6",
            Dirty1A => "Dirty 1 A",
            Dirty1B => "Dirty 1 B",
            Dirty1C => "Dirty 1 C",
            Dirty2A => "Dirty 2 A",
            Dirty2B => "Dirty 2 B",
            Dirty2C => "Dirty 2 C",
            Dirty3A => "Dirty 3 A",
            Dirty3B => "Dirty 3 B",
            Dirty3C => "Dirty 3 C",
            Dirty4A => "Dirty 4 A",
            Dirty4B => "Dirty 4 B",
            Dirty4C => "Dirty 4 C",
            Dirty5A => "Dirty 5 A",
            Dirty5B => "Dirty 5 B",
            Dirty5C => "Dirty 5 C",
            Dirty6A => "Dirty 6 A",
            Dirty6B => "Dirty 6 B",
            Dirty6C => "Dirty 6 C",
            Dirty7A => "Dirty 7 A",
            Dirty7B => "Dirty 7 B",
            Dirty7C => "Dirty 7 C",
            Dirty8A => "Dirty 8 A",
            Dirty8B => "Dirty 8 B",
            Dirty8C => "Dirty 8 C",
            Gate1 => "Gate 1",
            Gate2 => "Gate 2",
            Gate3 => "Gate 3",
            Gate4 => "Gate 4",
            Duck1 => "Duck 1",
            Duck2 => "Duck 2",
            Duck3 => "Duck 3",
        };
        f.write_str(s)
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename = "PARAM")]
pub struct Param {
    pub id: String,
    pub value: Option<String>,
}

impl Param {
    fn value_into<T: FromStr>(&self) -> Option<T> {
        self.value.as_ref().and_then(|v| v.parse::<T>().ok())
    }

    /// Convert the value into a boolean. Babylon stores booleans as a floating
    /// point value (!) so traditional conversion methods don't work.
    fn value_bool(&self) -> Option<bool> {
        self.value_into().map(|v: f64| (v - 1.0).abs() < 0.0000001)
    }

    /// Convert the value into a `i32`. Babylon stores integers as a floating
    /// point values is several places.
    fn value_i32(&self) -> Option<i32> {
        self.value_into().map(|v: f64| v as i32)
    }

    fn value_u32(&self) -> Option<u32> {
        self.value_into().map(|v: f64| v as u32)
    }
}

/// The Babylon preset as it's stored in XML. This is converted to a [`Preset`].
#[derive(Debug, Deserialize, Serialize)]
struct PluginParamTree {
    // EnvLock, FilterLock, FXLock, PortamentoLock and TunerLock are not read because
    // they effect the next preset loaded in Babylon and not the current preset.  It is
    // unclear why they would be stored in the preset file in the first place.
    #[serde(rename = "Scale")]
    scale: u32,

    #[serde(rename = "CustomScale")]
    custom_scale: u32,

    #[serde(rename = "Root")]
    root_key: u32,

    /// The preset ID doesn't appear to have a logical use. The preset IDs
    /// in the factory presets don't seem to follow any pattern.
    #[serde(rename = "PresetID")]
    preset_id: Option<i32>, // -1 appears in some

    /// The preset folder doesn't appear to have a logical use. The folder
    /// numbers in the factory presets don't seem to follow any pattern.
    #[serde(rename = "PresetFolder")]
    preset_folder: Option<u32>,

    #[serde(rename = "PresetName")]
    preset_name: String,

    #[serde(rename = "PresetInfo")]
    preset_info: String,

    #[serde(rename = "FX_Order_0")]
    fx_order0: Option<u32>,

    #[serde(rename = "FX_Order_1")]
    fx_order1: Option<u32>,

    #[serde(rename = "FX_Order_2")]
    fx_order2: Option<u32>,

    #[serde(rename = "FX_Order_3")]
    fx_order3: Option<u32>,

    #[serde(rename = "FX_Order_4")]
    fx_order4: Option<u32>,

    #[serde(rename = "FX_Order_5")]
    fx_order5: Option<u32>,

    #[serde(rename = "FX_Order_6")]
    fx_order6: Option<u32>,

    #[serde(rename = "PARAM", default)]
    params: Vec<Param>,
}

impl PluginParamTree {
    /// Remove a parameter with the given identifier, returning it.
    fn remove(&mut self, id: &str) -> Option<Param> {
        let index_result = self.params.iter().position(|param| param.id == id);
        match index_result {
            Some(index) => Some(self.params.remove(index)),
            None => None,
        }
    }

    fn remove_or<T: FromStr>(&mut self, id: &str, default: T) -> T {
        match self.remove(id) {
            Some(param) => param.value_into().unwrap_or(default),
            None => default,
        }
    }

    fn remove_bool_or(&mut self, id: &str, default: bool) -> bool {
        match self.remove(id) {
            Some(param) => param.value_bool().unwrap_or(default),
            None => default,
        }
    }

    fn remove_milliseconds_or(&mut self, id: &str, default: f64) -> Time {
        let millis: f64 = match self.remove(id) {
            Some(param) => param.value_into().unwrap_or(default),
            None => default,
        };
        Time::new::<millisecond>(millis)
    }

    fn remove_percent_or(&mut self, id: &str, default: f64) -> Ratio {
        let pct: f64 = match self.remove(id) {
            Some(param) => param.value_into().unwrap_or(default),
            None => default,
        };
        Ratio::new::<percent>(pct)
    }

    fn remove_u32_or(&mut self, id: &str, default: u32) -> u32 {
        match self.remove(id) {
            Some(param) => param.value_u32().unwrap_or(default),
            None => default,
        }
    }

    fn remove_i32_or(&mut self, id: &str, default: i32) -> i32 {
        match self.remove(id) {
            Some(param) => param.value_i32().unwrap_or(default),
            None => default,
        }
    }
}

/// Converted from a `PluginParamTree` into a more usable model.
#[derive(Debug)]
pub struct Preset {
    pub name: String,
    pub description: Option<String>,

    /// The master volume from 0.0 to 1.0. The value 0.0 maps to -inf dB,
    /// 0.5 maps to 0.0 dB and 1.0 maps to 10.0 dB.
    #[doc(alias = "main_volume")]
    pub master_volume_normalized: f64,

    pub polyphony: u32,
    pub portamento_mode: PortamentoMode,
    pub midi_play_mode: MidiPlayMode,
    pub glide: f64,
    pub velocity_curve: f64,
    pub key_track_curve: f64,
    pub pitch_bend_range: f64,

    /// Limit the output to 0 dB using soft clipping
    pub limit_enabled: bool,
    pub tuning: Tuning,
    pub envelope: Envelope,
    pub envelope_curve: f64,
    pub filter: Filter,
    pub filter_envelope_curve: f64,

    // Oscillators
    pub oscillators: Vec<Oscillator>,

    /// Sync oscillator 2 to oscillator 1.  Oscillator 2 resets when oscillator 1 does.
    pub hard_sync: bool,
    pub noise: Noise,

    // Modulators
    pub lfos: Vec<Lfo>,
    pub mod_envelopes: Vec<ModulatorEnvelope>,
    pub vibrato: Vibrato,
    pub matrix: Vec<MatrixItem>,

    // Effects
    pub effect_order: Vec<EffectType>,
    pub chorus: Chorus,
    pub delay: Delay,
    pub distortion: Distortion,
    pub equalizer: Equalizer,
    pub effect_filter: Filter,
    pub lofi: LoFi,
    pub reverb: Reverb,
}

impl Preset {
    /// Where in the effect order the effect type occurs.
    pub fn effect_position(&self, effect_type: EffectType) -> Option<u8> {
        self.effect_order
            .iter()
            .position(|e| e == &effect_type)
            .map(|pos| pos as u8)
    }

    pub fn read_file<P: AsRef<Path>>(path: P) -> Result<Preset, Error> {
        let input = File::open(&path)?;
        let reader = BufReader::new(input);

        let mut param_tree: PluginParamTree = match from_reader(reader) {
            Ok(param_tree) => param_tree,
            Err(error) => return Err(Error::new(ErrorKind::InvalidData, error)),
        };

        let name = param_tree.preset_name.clone();
        let description: String = param_tree.preset_info.clone();
        let description = (description.as_str() != PRESET_INFO_DEFAULT).then_some(description);

        let envelope = Envelope {
            attack: param_tree.remove_milliseconds_or("EnvAttack", 2.0),
            attack_curve: param_tree.remove_or("AttCurveType", 0.07),
            decay: param_tree.remove_milliseconds_or("EnvDecay", 150.0),
            decay_falloff: param_tree.remove_or("DecCurveType", 0.07),
            sustain: param_tree.remove_percent_or("EnvSustain", 0.9),
            release: param_tree.remove_milliseconds_or("EnvRelease", 4.0),
            release_falloff: param_tree.remove_or("RelCurveType", 0.07),
        };

        let mut tunings = [0.0; 12];
        tunings[0] = param_tree.remove_or("TuneA", 0.0);
        tunings[1] = param_tree.remove_or("TuneASharp", 0.0);
        tunings[2] = param_tree.remove_or("TuneB", 0.0);
        tunings[3] = param_tree.remove_or("TuneC", 0.0);
        tunings[4] = param_tree.remove_or("TuneCSharp", 0.0);
        tunings[5] = param_tree.remove_or("TuneD", 0.0);
        tunings[6] = param_tree.remove_or("TuneDSharp", 0.0);
        tunings[7] = param_tree.remove_or("TuneE", 0.0);
        tunings[8] = param_tree.remove_or("TuneF", 0.0);
        tunings[9] = param_tree.remove_or("TuneFSharp", 0.0);
        tunings[10] = param_tree.remove_or("TuneG", 0.0);
        tunings[11] = param_tree.remove_or("TuneGSharp", 0.0);
        let tuning = Tuning {
            transpose: param_tree.remove_or("Transpose", 0.0),
            root_key: param_tree.root_key,
            scale: param_tree.scale,
            tunings,
        };

        // No idea what this is for. There isn't any difference in the interface regardless
        // of the value. "PCH" is often short for "pitch".
        let _ = param_tree.remove_or("PCH", 0.0);

        let filter_envelope = Envelope {
            attack: param_tree.remove_milliseconds_or("FilterEnvAttack", 2.0),
            attack_curve: param_tree.remove_or("FilterAttCurveType", 0.07),
            decay: param_tree.remove_milliseconds_or("FilterEnvDecay", 150.0),
            decay_falloff: param_tree.remove_or("FilterDecCurveType", 0.07),
            sustain: param_tree.remove_percent_or("FilterEnvSustain", 0.02),
            release: param_tree.remove_milliseconds_or("FilterEnvRelease", 23.0),
            release_falloff: param_tree.remove_or("FilterRelCurveType", 0.07),
        };

        let filter = Filter {
            enabled: param_tree.remove_bool_or("FilterSwitch", false),
            mode: FilterMode::from_or(
                param_tree.remove_u32_or("FilterType", FilterMode::LowPass as u32),
                FilterMode::LowPass,
            ),
            resonance: param_tree.remove_or("FilterRes", 0.0),
            cutoff_frequency: param_tree.remove_or("FilterCut", 1.0) * 100.0,
            key_tracking: param_tree.remove_or("FilterKey", 0.0),
            envelope: filter_envelope,
            envelope_amount: param_tree.remove_or("FilterEnv", 0.0),
            effect_enabled: param_tree.remove_bool_or("FilterDriveSwitch", false),
            effect_mode: FilterEffectMode::from_or(
                param_tree.remove_u32_or("FilterDriveType", FilterEffectMode::Off as u32),
                FilterEffectMode::Off,
            ),
            effect_amount: param_tree.remove_or("FilterDrive", 0.5),
        };

        //
        // Oscillators
        //

        let mut oscillators = Vec::new();
        for index in 1..=3 {
            let oscillator = Oscillator {
                enabled: param_tree.remove_bool_or(format!("OSCSwitch_{}", index).as_str(), true),
                waveform: Waveform::from_or(
                    param_tree.remove_u32_or(
                        format!("OSCWaveType_{}", index).as_str(),
                        Waveform::Sine as u32,
                    ),
                    Waveform::Sine,
                ),
                invert: param_tree.remove_bool_or(format!("OSCInvert_{}", index).as_str(), false),
                pan: param_tree.remove_or(format!("OSCPan_{}", index).as_str(), 0.5),
                phase: param_tree.remove_or(format!("OSCPhase_{}", index).as_str(), 0.0),
                pitch: param_tree.remove_or(format!("OSCPitch_{}", index).as_str(), 0.0),
                fine_tuning: param_tree.remove_i32_or(format!("OSCFine_{}", index).as_str(), 0),
                semitone_tuning: param_tree.remove_i32_or(format!("OSCSemi_{}", index).as_str(), 0),
                octave_tuning: param_tree.remove_i32_or(format!("OSCOctave_{}", index).as_str(), 0),
                reverse: param_tree.remove_bool_or(format!("OSCReverse_{}", index).as_str(), false),
                free_run: param_tree
                    .remove_bool_or(format!("OSCFreeRun_{}", index).as_str(), false),
                sync_all: param_tree
                    .remove_bool_or(format!("OSCSyncAll_{}", index).as_str(), false),
                volume: param_tree.remove_or(format!("OSCVol_{}", index).as_str(), 0.294),
                unison: Unison {
                    voices: param_tree.remove_u32_or(format!("OSCNumVoice_{}", index).as_str(), 1),
                    detune: param_tree.remove_or(format!("OSCDetune_{}", index).as_str(), 0.2),
                    spread: param_tree.remove_or(format!("OSCSpread_{}", index).as_str(), 0.5),
                    mix: param_tree.remove_or(format!("OSCUniMix_{}", index).as_str(), 1.0),
                },
                am_enabled: param_tree
                    .remove_bool_or(format!("OSCAMSwitch_{}", index).as_str(), false),
                am_amount: param_tree.remove_or(format!("OSCAM_{}", index).as_str(), 0.0),
                fm_enabled: param_tree
                    .remove_bool_or(format!("OSCFMSwitch_{}", index).as_str(), false),
                fm_amount: param_tree.remove_or(format!("OSCFM_{}", index).as_str(), 0.0),
                rm_enabled: param_tree
                    .remove_bool_or(format!("OSCRMSwitch_{}", index).as_str(), false),
                rm_amount: param_tree.remove_or(format!("OSCRM_{}", index).as_str(), 0.0),
            };
            oscillators.push(oscillator);
        }

        let noise = Noise {
            enabled: param_tree.remove_bool_or("OSCSwitch_N", false),
            width: param_tree.remove_or("OSCWidth_N", 1.0),
            pan: param_tree.remove_or("OSCPan_N", 0.5),
            volume: param_tree.remove_or("OSCVol_N", 0.32),
        };

        //
        // Modulators
        //

        let lfo1 = Lfo {
            enabled: param_tree.remove_bool_or("LFOSwitch_1", false),
            waveform: Waveform::from_or(
                param_tree.remove_u32_or("LFOWaveType_1", Waveform::Sine as u32),
                Waveform::Sine,
            ),
            sync: param_tree.remove_bool_or("LFOSync_1", true),
            invert: param_tree.remove_bool_or("LFOInvert_1", false),
            reverse: param_tree.remove_bool_or("LFOReverse_1", false),
            mono: param_tree.remove_bool_or("LFOMono_1", false),
            free_run: param_tree.remove_bool_or("LFOFreeRun_1", false),
            frequency: param_tree.remove_or("LFOFreq_1", 0.35),
            phase: param_tree.remove_or("LFOPhase_1", 0.0),
        };

        let lfo2 = Lfo {
            enabled: param_tree.remove_bool_or("LFOSwitch_2", false),
            waveform: Waveform::from_or(
                param_tree.remove_u32_or("LFOWaveType_2", Waveform::Sine as u32),
                Waveform::Sine,
            ),
            sync: param_tree.remove_bool_or("LFOSync_2", true),
            invert: param_tree.remove_bool_or("LFOInvert_2", false),
            reverse: param_tree.remove_bool_or("LFOReverse_2", false),
            mono: param_tree.remove_bool_or("LFOMono_2", false),
            free_run: param_tree.remove_bool_or("LFOFreeRun_2", false),
            frequency: param_tree.remove_or("LFOFreq_2", 0.35),
            phase: param_tree.remove_or("LFOPhase_2", 0.0),
        };

        let lfos = vec![lfo1, lfo2];

        let mod_envelope1 = ModulatorEnvelope {
            enabled: param_tree.remove_bool_or("ModEnvSwitch_1", false),
            curve: param_tree.remove_or("ModEnvCurveType_1", 0.14),
            envelope: Envelope {
                attack: param_tree.remove_milliseconds_or("ModEnvAttack_1", 1.0),
                attack_curve: param_tree.remove_or("ModAttCurveType_1", 0.07),
                decay: param_tree.remove_milliseconds_or("ModEnvDecay_1", 150.0),
                decay_falloff: param_tree.remove_or("ModDecCurveType_1", 0.07),
                sustain: param_tree.remove_percent_or("ModEnvSustain_1", 1.9),
                release: param_tree.remove_milliseconds_or("ModEnvRelease_1", 1.0),
                release_falloff: param_tree.remove_or("ModRelCurveType_1", 0.07),
            },
        };
        let mod_envelope2 = ModulatorEnvelope {
            enabled: param_tree.remove_bool_or("ModEnvSwitch_2", false),
            curve: param_tree.remove_or("ModEnvCurveType_2", 0.14),
            envelope: Envelope {
                attack: param_tree.remove_milliseconds_or("ModEnvAttack_2", 1.0),
                attack_curve: param_tree.remove_or("ModAttCurveType_2", 0.07),
                decay: param_tree.remove_milliseconds_or("ModEnvDecay_2", 150.0),
                decay_falloff: param_tree.remove_or("ModDecCurveType_2", 0.07),
                sustain: param_tree.remove_percent_or("ModEnvSustain_2", 0.9),
                release: param_tree.remove_milliseconds_or("ModEnvRelease_2", 1.0),
                release_falloff: param_tree.remove_or("ModRelCurveType_2", 0.07),
            },
        };
        let mod_envelopes = vec![mod_envelope1, mod_envelope2];

        let vibrato = Vibrato {
            enabled: param_tree.remove_bool_or("VibSwitch", false),
            attack: param_tree.remove_or("VibAttack", 232.0),
            frequency: param_tree.remove_or("VibFrequency", 6.1),
            delay: param_tree.remove_or("VibDelay", 232.0),
        };

        let mut matrix = Vec::new();
        for index in 1..=MODULATION_MATRIX_SIZE {
            matrix.push(MatrixItem {
                source: param_tree.remove_or(
                    format!("MatrixSource_{}", index).as_str(),
                    if index == 1 { 7 } else { 0 },
                ),
                target: param_tree.remove_or(
                    format!("MatrixTarget_{}", index).as_str(),
                    if index == 1 { 2 } else { 0 },
                ),
                amount: param_tree.remove_or(
                    format!("MatrixAmount_{}", index).as_str(),
                    if index == 1 { 1.0 } else { 0.0 },
                ),
            });
        }

        //
        // Effects
        //

        let effect_type_ids = [
            param_tree.fx_order0.unwrap_or(0),
            param_tree.fx_order1.unwrap_or(1),
            param_tree.fx_order2.unwrap_or(2),
            param_tree.fx_order3.unwrap_or(3),
            param_tree.fx_order4.unwrap_or(4),
            param_tree.fx_order5.unwrap_or(5),
            param_tree.fx_order6.unwrap_or(6),
        ];
        let mut effect_order = Vec::with_capacity(effect_type_ids.len());
        for effect_type_id in effect_type_ids.iter() {
            match EffectType::try_from(*effect_type_id) {
                Ok(effect) => effect_order.push(effect),
                Err(msg) => return Err(Error::new(ErrorKind::InvalidData, msg)),
            }
        }

        let chorus = Chorus {
            enabled: param_tree.remove_bool_or("ChorusSwitch", false),
            depth: param_tree.remove_or("ChorusDepth", 0.5),
            mix: param_tree.remove_or("ChorusMix", 0.5),
            pre_delay: param_tree.remove_or("ChorusPdelay", 0.5),
            ratio: param_tree.remove_or("ChorusRatio", 0.5),
        };

        let delay = Delay {
            enabled: param_tree.remove_bool_or("DelaySwitch", false),
            ping_pong: param_tree.remove_bool_or("DelayMode", false),
            feedback: param_tree.remove_or("DelayFeed", 0.3),
            filter: param_tree.remove_or("DelayLP", 0.0),
            sync: param_tree.remove_bool_or("DelaySync", true),
            time: param_tree.remove_or("DelayTime", 0.17),
            mix: param_tree.remove_or("DelayMix", 0.2),
        };

        let distortion = Distortion {
            enabled: param_tree.remove_bool_or("DistSwitch", false),
            gain: param_tree.remove_or("DistGain", 0.2),
        };

        let equalizer = Equalizer {
            enabled: param_tree.remove_bool_or("EQSwitch", false),
            high_gain: param_tree.remove_or("EQHigh", Ratio::new::<percent>(0.5)),
            low_gain: param_tree.remove_or("EQLow", Ratio::new::<percent>(0.5)),
            mid_gain: param_tree.remove_or("EQMid", Ratio::new::<percent>(0.5)),
        };

        let effect_filter = Filter {
            enabled: param_tree.remove_bool_or("FXFilterSwitch", false),
            mode: FilterMode::from_or(
                param_tree.remove_u32_or("FXFilterType", FilterMode::LowPass as u32),
                FilterMode::LowPass,
            ),
            resonance: param_tree.remove_or("FXFilterRes", 0.0),
            cutoff_frequency: param_tree.remove_or("FXFilterCut", 1.0),
            key_tracking: 0.0,
            envelope: Envelope {
                attack: Time::new::<second>(-1.01),
                attack_curve: -1.0,
                decay: Time::new::<second>(-1.1),
                decay_falloff: -1.0,
                sustain: Ratio::zero(),
                release: Time::new::<second>(-1.1),
                release_falloff: -1.0,
            },
            envelope_amount: 1.0,
            effect_enabled: false,
            effect_mode: FilterEffectMode::Off,
            effect_amount: 0.0,
        };

        let lofi = LoFi {
            enabled: param_tree.remove_bool_or("LoFiSwitch", false),
            bitrate: param_tree.remove_or("LoFiBitRate", 1.0),
            sample_rate: param_tree.remove_or("LoFiSampleRate", 1.0),
            mix: param_tree.remove_or("LoFiMix", 1.0),
        };

        let reverb = Reverb {
            enabled: param_tree.remove_bool_or("ReverbSwitch", false),
            dampen: param_tree.remove_or("ReverbDamp", 0.3),
            room: param_tree.remove_or("ReverbRoom", 0.3),
            filter: param_tree.remove_or("ReverbLP", 0.0),
            width: param_tree.remove_or("ReverbWidth", 0.8),
            mix: param_tree.remove_or("ReverbMix", 0.2),
        };

        let preset = Preset {
            name,
            description,
            master_volume_normalized: param_tree.remove_or("MainVol", 0.0),
            polyphony: param_tree.remove_or("MaxVoices", 8),
            portamento_mode: PortamentoMode::from_or(
                param_tree.remove_u32_or("PortaMode", PortamentoMode::Poly as u32),
                PortamentoMode::Poly,
            ),
            midi_play_mode: MidiPlayMode::from_or(
                param_tree.remove_u32_or("MidiPlayMode", MidiPlayMode::Normal as u32),
                MidiPlayMode::Normal,
            ),
            glide: param_tree.remove_or("Glide", 30.0),
            velocity_curve: param_tree.remove_or("VeloCurve", 0.5),
            key_track_curve: param_tree.remove_or("KeyTrackCurve", 0.0),
            pitch_bend_range: param_tree.remove_or("PBRange", 2.0),
            limit_enabled: param_tree.remove_bool_or("LimitSwitch", false),
            tuning,
            envelope,
            envelope_curve: param_tree.remove_or("EnvCurveType", 0.14),
            filter,
            filter_envelope_curve: param_tree.remove_or("FilterEnvCurveType", 0.14),

            // Oscillators
            oscillators,
            hard_sync: param_tree.remove_bool_or("OSCSync21", false),
            noise,

            // Modulators
            lfos,
            vibrato,
            mod_envelopes,
            matrix,

            // Effects
            effect_order,
            chorus,
            delay,
            distortion,
            equalizer,
            effect_filter,
            lofi,
            reverb,
        };

        for param in &param_tree.params {
            warn!(
                "Unrecognized parameter while reading {}, parameter {} is {:?}",
                path.as_ref().to_string_lossy(),
                param.id,
                param.value
            );
        }

        Ok(preset)
    }
}

#[cfg(test)]
mod test {
    use std::io::Result;
    use std::path::Path;

    use approx::assert_relative_eq;
    use uom::si::ratio::percent;

    use super::effect::{EffectType, FilterEffectMode, FilterMode};
    use super::*;

    fn read_preset(filename: &str) -> Result<Preset> {
        let path = &Path::new("tests").join(&filename);
        Preset::read_file(path)
    }

    /// Check defaults.
    #[test]
    fn init() {
        let preset = read_preset("init-1.0.2.bab").unwrap();
        assert_eq!(preset.master_volume_normalized, 0.5); // 0 dB
        assert_eq!(preset.polyphony, 8);
        assert_eq!(preset.portamento_mode, PortamentoMode::Poly);
        assert_eq!(preset.midi_play_mode, MidiPlayMode::Normal);
        assert_eq!(preset.velocity_curve, 0.5);
        assert_eq!(preset.key_track_curve, 0.0);
        assert_eq!(preset.pitch_bend_range, 2.0);
        assert!(!preset.limit_enabled);
        assert_relative_eq!(preset.glide, 30.0, epsilon = 0.0001);

        assert_eq!(preset.name, "init".to_owned());
        assert!(preset.description.is_none());

        let envelope = &preset.envelope;
        assert_relative_eq!(envelope.attack.get::<millisecond>(), 2.0, epsilon = 0.0001);
        assert_relative_eq!(envelope.attack_curve, 0.07, epsilon = 0.0001);
        assert_relative_eq!(envelope.decay.get::<millisecond>(), 150.0, epsilon = 0.0001);
        assert_relative_eq!(envelope.decay_falloff, 0.07, epsilon = 0.0001);
        assert_relative_eq!(envelope.sustain.get::<percent>(), 0.9, epsilon = 0.0001);
        assert_relative_eq!(envelope.release.get::<millisecond>(), 4.0, epsilon = 0.0001);
        assert_relative_eq!(envelope.release_falloff, 0.07, epsilon = 0.0001);
        assert_relative_eq!(preset.envelope_curve, 0.14, epsilon = 0.0001);

        let tuning = &preset.tuning;
        assert_eq!(tuning.transpose, 0.0);
        assert_eq!(tuning.scale, 0);
        assert_eq!(tuning.root_key, 0);
        let tunings = tuning.tunings;
        assert_eq!(tunings, [0.0_f64; 12]);

        let filter = &preset.filter;
        assert!(!filter.enabled);
        assert_eq!(filter.mode, FilterMode::LowPass);
        assert_relative_eq!(filter.resonance, 0.0, epsilon = 0.0001);
        assert_relative_eq!(filter.key_tracking, 0.0, epsilon = 0.0001);
        assert_relative_eq!(filter.cutoff_frequency, 100.0, epsilon = 0.0001);
        assert_relative_eq!(filter.envelope_amount, 0.0, epsilon = 0.0001);
        assert!(!filter.effect_enabled);
        assert_relative_eq!(filter.effect_amount, 0.5, epsilon = 0.0001);
        assert_eq!(filter.effect_mode, FilterEffectMode::Off);

        let filter_env = &filter.envelope;
        assert_relative_eq!(
            filter_env.attack.get::<millisecond>(),
            2.0,
            epsilon = 0.0001
        );
        assert_relative_eq!(filter_env.attack_curve, 0.07, epsilon = 0.0001);
        assert_relative_eq!(
            filter_env.decay.get::<millisecond>(),
            150.0,
            epsilon = 0.0001
        );
        assert_relative_eq!(filter_env.decay_falloff, 0.07, epsilon = 0.0001);
        assert_relative_eq!(filter_env.sustain.get::<percent>(), 0.02, epsilon = 0.0001);
        assert_relative_eq!(
            filter_env.release.get::<millisecond>(),
            4.0,
            epsilon = 0.0001
        );
        assert_relative_eq!(filter_env.release_falloff, 0.07, epsilon = 0.0001);
        assert_relative_eq!(preset.filter_envelope_curve, 0.14, epsilon = 0.0001);

        //
        // Oscillators
        //

        assert_eq!(preset.oscillators.len(), 3);
        assert!(preset.oscillators[0].enabled);
        assert!(!preset.oscillators[1].enabled);
        assert!(!preset.oscillators[2].enabled);
        for osc in &preset.oscillators {
            assert!(!osc.invert);
            assert!(!osc.reverse);
            assert!(!osc.free_run);
            assert!(!osc.sync_all);

            assert!(!osc.am_enabled);
            assert_eq!(osc.am_amount, 0.0);
            assert!(!osc.fm_enabled);
            assert_eq!(osc.fm_amount, 0.0);
            assert!(!osc.rm_enabled);
            assert_eq!(osc.rm_amount, 0.0);

            assert_eq!(osc.waveform, Waveform::Sine);

            assert_relative_eq!(osc.pan, 0.5, epsilon = 0.0001);
            assert_relative_eq!(osc.phase, 0.0, epsilon = 0.0001);
            assert_relative_eq!(osc.volume, 0.5, epsilon = 0.0001);

            assert_relative_eq!(osc.pitch, 0.0, epsilon = 0.0001);
            assert_eq!(osc.fine_tuning, 0);
            assert_eq!(osc.semitone_tuning, 0);
            assert_eq!(osc.octave_tuning, 0);

            let unison = &osc.unison;
            assert_eq!(unison.voices, 1);
            assert_relative_eq!(unison.detune, 0.2, epsilon = 0.0001);
            assert_relative_eq!(unison.spread, 0.5, epsilon = 0.0001);
            assert_relative_eq!(unison.mix, 1.0, epsilon = 0.0001);
        }

        assert!(!preset.hard_sync);

        let noise = &preset.noise;
        assert!(!noise.enabled);
        assert_relative_eq!(noise.width, 1.0, epsilon = 0.0001);
        assert_relative_eq!(noise.pan, 0.5, epsilon = 0.0001);
        assert_relative_eq!(noise.volume, 0.32, epsilon = 0.0001);

        //
        // Modulators
        //

        assert_eq!(preset.lfos.len(), 2);
        for lfo in &preset.lfos {
            assert!(!lfo.enabled);
            assert_eq!(lfo.waveform, Waveform::Sine);
            assert!(lfo.sync);
            assert!(!lfo.invert);
            assert!(!lfo.reverse);
            assert!(!lfo.mono);
            assert!(!lfo.free_run);
            assert_relative_eq!(lfo.frequency, 0.35, epsilon = 0.0001);
            assert_relative_eq!(lfo.phase, 0.0, epsilon = 0.0001);
        }

        assert_eq!(preset.mod_envelopes.len(), 2);
        for mod_envelope in &preset.mod_envelopes {
            assert!(!mod_envelope.enabled);
            assert_relative_eq!(mod_envelope.curve, 0.14, epsilon = 0.0001);
            let env = &mod_envelope.envelope;
            assert_relative_eq!(env.attack.get::<millisecond>(), 1.0, epsilon = 0.0001);
            assert_relative_eq!(env.attack_curve, 0.07, epsilon = 0.0001);
            assert_relative_eq!(env.decay.get::<millisecond>(), 150.0, epsilon = 0.0001);
            assert_relative_eq!(env.decay_falloff, 0.07, epsilon = 0.0001);
            assert_relative_eq!(env.sustain.get::<percent>(), 0.9, epsilon = 0.0001);
            assert_relative_eq!(env.release.get::<millisecond>(), 1.0, epsilon = 0.0001);
            assert_relative_eq!(env.release_falloff, 0.07, epsilon = 0.0001);
        }

        let vibrato = &preset.vibrato;
        assert!(!vibrato.enabled);
        assert_relative_eq!(vibrato.attack, 232.0, epsilon = 0.0001);
        assert_relative_eq!(vibrato.delay, 232.0, epsilon = 0.0001);
        assert_relative_eq!(vibrato.frequency, 6.1, epsilon = 0.0001);

        assert_eq!(preset.matrix[0].source, 7);
        assert_eq!(preset.matrix[0].target, 2);
        assert_eq!(preset.matrix[0].amount, 1.0);
        for index in 1..MODULATION_MATRIX_SIZE {
            assert_eq!(preset.matrix[index].source, 0);
            assert_eq!(preset.matrix[index].target, 0);
            assert_eq!(preset.matrix[index].amount, 0.0);
        }

        //
        // Effects
        //

        let expected_effect_order: Vec<EffectType> = EffectType::iter().collect();
        assert_eq!(&preset.effect_order, &expected_effect_order);

        let chorus = &preset.chorus;
        assert!(!chorus.enabled);
        assert_eq!(chorus.depth, 0.5);
        assert_eq!(chorus.mix, 0.5);
        assert_eq!(chorus.pre_delay, 0.5);
        assert_eq!(chorus.ratio, 0.5);

        let delay = &preset.delay;
        assert!(!delay.enabled);
        assert!(!delay.ping_pong);
        assert!(delay.sync);
        assert_relative_eq!(delay.time, 0.17, epsilon = 0.0001);
        assert_relative_eq!(delay.feedback, 0.3, epsilon = 0.0001);
        assert_relative_eq!(delay.filter, 0.0, epsilon = 0.0001);
        assert_relative_eq!(delay.mix, 0.2, epsilon = 0.0001);

        let distortion = &preset.distortion;
        assert!(!distortion.enabled);
        assert_relative_eq!(distortion.gain, 0.2, epsilon = 0.0001);

        let effect_filter = &preset.effect_filter;
        assert!(!effect_filter.enabled);
        assert_eq!(effect_filter.mode, FilterMode::LowPass);
        assert_eq!(effect_filter.effect_mode, FilterEffectMode::Off);
        assert_relative_eq!(effect_filter.cutoff_frequency, 0.5, epsilon = 0.0001);
        assert_relative_eq!(effect_filter.resonance, 0.1, epsilon = 0.0001);
        assert_relative_eq!(effect_filter.resonance, 0.1, epsilon = 0.0001);
        assert_relative_eq!(effect_filter.key_tracking, 0.0, epsilon = 0.0001);
        assert_relative_eq!(effect_filter.effect_amount, 0.0, epsilon = 0.0001);
        assert_relative_eq!(preset.filter_envelope_curve, 0.14, epsilon = 0.0001);

        let equalizer = preset.equalizer;
        assert!(!equalizer.enabled);
        assert_eq!(equalizer.low_gain.get::<percent>(), 0.5);
        assert_eq!(equalizer.mid_gain.get::<percent>(), 0.5);
        assert_eq!(equalizer.high_gain.get::<percent>(), 0.5);

        let lofi = &preset.lofi;
        assert!(!lofi.enabled);
        assert_relative_eq!(lofi.bitrate, 1.0, epsilon = 0.0001);
        assert_relative_eq!(lofi.sample_rate, 1.0, epsilon = 0.0001);
        assert_relative_eq!(lofi.mix, 1.0, epsilon = 0.0001);

        let reverb = &preset.reverb;
        assert!(!reverb.enabled);
        assert_relative_eq!(reverb.dampen, 0.3, epsilon = 0.0001);
        assert_relative_eq!(reverb.filter, 0.0, epsilon = 0.0001);
        assert_relative_eq!(reverb.room, 0.3, epsilon = 0.0001);
        assert_relative_eq!(reverb.width, 0.8, epsilon = 0.0001);
        assert_relative_eq!(reverb.mix, 0.2, epsilon = 0.0001);
    }

    #[test]
    fn envelopes() {
        let preset = read_preset("envelopes-1.0.2.bab").unwrap();

        // ADSR
        let envelope = &preset.envelope;
        assert_relative_eq!(envelope.attack.get::<millisecond>(), 1.0, epsilon = 0.00001);
        assert_relative_eq!(
            envelope.attack_curve,
            EnvelopeCurve::Linear.value(),
            epsilon = 0.00001
        );
        assert_relative_eq!(
            envelope.decay.get::<millisecond>(),
            15000.0,
            epsilon = 0.00001
        );
        assert_relative_eq!(
            envelope.decay_falloff,
            EnvelopeCurve::Exponential1.value(),
            epsilon = 0.00001
        );
        assert_relative_eq!(envelope.sustain.get::<percent>(), 0.42, epsilon = 0.00001);
        assert_relative_eq!(
            envelope.release.get::<millisecond>(),
            76.0,
            epsilon = 0.00001
        );
        assert_relative_eq!(
            envelope.release_falloff,
            EnvelopeCurve::Exponential2.value(),
            epsilon = 0.00001
        );

        // Modulator envelope 1
        let mod_envelope = &preset.mod_envelopes.get(0).unwrap();
        assert!(mod_envelope.enabled);
        let envelope = &mod_envelope.envelope;
        assert_relative_eq!(
            envelope.attack.get::<millisecond>(),
            748.0,
            epsilon = 0.00001
        );
        assert_relative_eq!(
            envelope.attack_curve,
            EnvelopeCurve::Pluck1.value(),
            epsilon = 0.00001
        );
        assert_relative_eq!(
            envelope.decay.get::<millisecond>(),
            150.0,
            epsilon = 0.00001
        );
        assert_relative_eq!(
            envelope.decay_falloff,
            EnvelopeCurve::Pluck2.value(),
            epsilon = 0.00001
        );
        assert_relative_eq!(envelope.sustain.get::<percent>(), 0.90, epsilon = 0.00001);
        assert_relative_eq!(
            envelope.release.get::<millisecond>(),
            1.0,
            epsilon = 0.00001
        );
        assert_relative_eq!(
            envelope.release_falloff,
            EnvelopeCurve::Pluck3.value(),
            epsilon = 0.00001
        );

        // Modulator envelope 2
        // NOTE: Bug report send to W. A. Productions on 2021-10-21 showing the curve types for
        // modulator 2 don't save properly. The labels for attack, decay and release always show
        // "L2" but the popup menu shows a different selection. This may also apply to the Filter
        // envelope.
        let mod_envelope = &preset.mod_envelopes.get(1).unwrap();
        assert!(!mod_envelope.enabled);
        let envelope = &mod_envelope.envelope;
        assert_relative_eq!(envelope.attack.get::<millisecond>(), 1.0, epsilon = 0.00001);
        assert_relative_eq!(
            envelope.attack_curve,
            EnvelopeCurve::Logarithmic2.value(),
            epsilon = 0.00001
        );
        assert_relative_eq!(envelope.decay.get::<millisecond>(), 2.0, epsilon = 0.00001);
        // assert_relative_eq!(envelope.decay_falloff, EnvelopeCurve::DoubleCurve1.value(), epsilon = 0.00001);
        assert_relative_eq!(envelope.sustain.get::<percent>(), 0.0, epsilon = 0.00001);
        assert_relative_eq!(
            envelope.release.get::<millisecond>(),
            1.0,
            epsilon = 0.00001
        );
        // assert_relative_eq!(envelope.release_falloff, EnvelopeCurve::DoubleCurve2.value(), epsilon = 0.00001);

        // Filter envelope
        let envelope = &preset.filter.envelope;
        assert_relative_eq!(envelope.attack.get::<millisecond>(), 2.0, epsilon = 0.00001);
        // assert_relative_eq!(envelope.attack_curve, EnvelopeCurve::Logarithmic1.value(), epsilon = 0.00001);
        assert_relative_eq!(
            envelope.decay.get::<millisecond>(),
            150.0,
            epsilon = 0.00001
        );
        // assert_relative_eq!(envelope.decay_falloff, EnvelopeCurve::Linear.value(), epsilon = 0.00001);
        assert_relative_eq!(envelope.sustain.get::<percent>(), 0.02, epsilon = 0.00001);
        assert_relative_eq!(
            envelope.release.get::<millisecond>(),
            4.0,
            epsilon = 0.00001
        );
        // assert_relative_eq!(envelope.release_falloff, EnvelopeCurve::Exponential4.value(), epsilon = 0.00001);
    }

    #[test]
    fn envelope_curves() {
        let preset = read_preset("envelope_curve-ae3-de4-rl1-1.0.3.bab").unwrap();
        assert_relative_eq!(
            preset.envelope.attack_curve,
            EnvelopeCurve::Exponential3.value(),
            epsilon = 0.0001
        );
        assert_relative_eq!(
            preset.envelope.decay_falloff,
            EnvelopeCurve::Exponential4.value(),
            epsilon = 0.0001
        );
        assert_relative_eq!(
            preset.envelope.release_falloff,
            EnvelopeCurve::Logarithmic1.value(),
            epsilon = 0.0001
        );

        let preset = read_preset("envelope_curve-ap3-dd1-rd2-1.0.3.bab").unwrap();
        assert_relative_eq!(
            preset.envelope.attack_curve,
            EnvelopeCurve::Pluck3.value(),
            epsilon = 0.0001
        );
        assert_relative_eq!(
            preset.envelope.decay_falloff,
            EnvelopeCurve::DoubleCurve1.value(),
            epsilon = 0.0001
        );
        assert_relative_eq!(
            preset.envelope.release_falloff,
            EnvelopeCurve::DoubleCurve2.value(),
            epsilon = 0.0001
        );
    }

    #[test]
    fn master_volume() {
        let preset = read_preset("master-volume-10-1.0.3.bab").unwrap();
        assert_eq!(preset.master_volume_normalized, 1.0);

        let preset = read_preset("master-volume--398-1.0.3.bab").unwrap();
        assert_relative_eq!(preset.master_volume_normalized, 0.007, epsilon = 0.001);

        let preset = read_preset("master-volume--inf-1.0.3.bab").unwrap();
        assert_eq!(preset.master_volume_normalized, 0.0);
    }

    #[test]
    fn midi_play_mode() {
        let preset = read_preset("playmode-cheat1-1.0.2.bab").unwrap();
        assert_eq!(preset.midi_play_mode, MidiPlayMode::Cheat1);
    }

    #[test]
    fn waveforms() {
        fn read_waveform_preset(filename: &str) -> Result<Preset> {
            let path = &Path::new("tests").join("waveforms").join(&filename);
            Preset::read_file(path)
        }

        let preset = read_waveform_preset("waveforms-formanta1-svoice1-organ1-1.0.3.bab").unwrap();
        assert_eq!(preset.oscillators[0].waveform, Waveform::FormantA1);
        assert_eq!(preset.oscillators[1].waveform, Waveform::SyntheticVoice1);
        assert_eq!(preset.oscillators[2].waveform, Waveform::Organ1);

        let preset =
            read_waveform_preset("waveforms-gritty1-gate1-sinefmkick12-1.0.3.bab").unwrap();
        assert_eq!(preset.oscillators[0].waveform, Waveform::Gritty1);
        assert_eq!(preset.oscillators[1].waveform, Waveform::Gate1);
        assert_eq!(preset.oscillators[2].waveform, Waveform::SineFmKick12);

        let preset = read_waveform_preset("waveforms-sine-triangle-saw-1.0.3.bab").unwrap();
        assert_eq!(preset.oscillators[0].waveform, Waveform::Sine);
        assert_eq!(preset.oscillators[1].waveform, Waveform::Triangle);
        assert_eq!(preset.oscillators[2].waveform, Waveform::Saw);

        let preset = read_waveform_preset("waveforms-square-pulse-voice1-1.0.3.bab").unwrap();
        assert_eq!(preset.oscillators[0].waveform, Waveform::Square);
        assert_eq!(preset.oscillators[1].waveform, Waveform::Pulse1);
        assert_eq!(preset.oscillators[2].waveform, Waveform::Voice1);
    }
}
