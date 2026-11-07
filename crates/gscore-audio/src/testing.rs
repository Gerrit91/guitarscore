//! A SoundFont built in code, for tests of this and other crates (feature
//! `test-support`): no third-party file needed. Not part of the normal API.

#![allow(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unwrap_used,
    clippy::expect_used
)]

fn chunk(id: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut out = id.to_vec();
    out.extend_from_slice(&u32::try_from(body.len()).unwrap().to_le_bytes());
    out.extend_from_slice(body);
    out
}

fn list(kind: &[u8; 4], chunks: &[Vec<u8>]) -> Vec<u8> {
    let mut body = kind.to_vec();
    for c in chunks {
        body.extend_from_slice(c);
    }
    chunk(b"LIST", &body)
}

fn name(text: &str) -> [u8; 20] {
    let mut out = [0; 20];
    let len = text.len().min(19);
    out[..len].copy_from_slice(&text.as_bytes()[..len]);
    out
}

fn u16s(values: &[u16]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

/// A SoundFont (`.sf2` bytes) named `bank` with one preset (bank 0, program 0) playing a
/// looped sine wave at A4 (440 Hz: 100 samples per period at 44 kHz). Every program falls
/// back to it.
pub fn sine_soundfont(bank: &str) -> Vec<u8> {
    sine_soundfont_at(bank, 0)
}

/// The same, its one preset in `bank` (128 is the percussion bank): a drum kit of one
/// sound, for the fork's per-part rendering.
pub fn sine_drum_soundfont(bank: &str) -> Vec<u8> {
    sine_soundfont_at(bank, 128)
}

/// [`sine_soundfont`]'s body, the preset in `preset_bank` and program 0.
fn sine_soundfont_at(bank: &str, preset_bank: u16) -> Vec<u8> {
    const PERIOD: u32 = 100;
    const PERIODS: u32 = 40;
    let samples = PERIOD * PERIODS;
    let mut wave: Vec<u8> = (0..samples)
        .flat_map(|i| {
            let phase = f64::from(i) / f64::from(PERIOD) * std::f64::consts::TAU;
            ((phase.sin() * 16_000.0) as i16).to_le_bytes()
        })
        .collect();
    // The format asks for 46 silent samples after each sample.
    wave.extend(std::iter::repeat_n(0, 46 * 2));

    let info = list(
        b"INFO",
        &[
            chunk(b"ifil", &u16s(&[2, 1])),
            chunk(b"INAM", &{
                let mut n = bank.as_bytes().to_vec();
                n.push(0);
                if n.len() % 2 == 1 {
                    n.push(0);
                }
                n
            }),
        ],
    );
    let sdta = list(b"sdta", &[chunk(b"smpl", &wave)]);

    let preset = |text: &str, program: u16, bag: u16| {
        let mut r = name(text).to_vec();
        r.extend(u16s(&[program, preset_bank, bag]));
        r.extend([0; 12]);
        r
    };
    let phdr = [preset("Sine", 0, 0), preset("EOP", 0, 1)].concat();
    let pbag = u16s(&[0, 0, 1, 0]);
    let pmod = vec![0; 10];
    // Instrument 0, then the terminal record.
    let pgen = u16s(&[41, 0, 0, 0]);
    let inst = [
        [name("Sine").to_vec(), u16s(&[0])].concat(),
        [name("EOI").to_vec(), u16s(&[1])].concat(),
    ]
    .concat();
    let ibag = u16s(&[0, 0, 2, 0]);
    let imod = vec![0; 10];
    // Loop continuously (sampleModes 54 = 1), sample 0 (sampleID 53, last).
    let igen = u16s(&[54, 1, 53, 0, 0, 0]);
    let header = |text: &str, start: u32, end: u32, loop_start: u32, loop_end: u32| {
        let mut r = name(text).to_vec();
        for v in [start, end, loop_start, loop_end, 44_000] {
            r.extend(v.to_le_bytes());
        }
        r.extend([69, 0]); // original pitch A4, no correction
        r.extend(u16s(&[0, 1])); // no link, mono
        r
    };
    let shdr = [
        header("Sine", 0, samples, PERIOD, samples - PERIOD),
        header("EOS", 0, 0, 0, 0),
    ]
    .concat();
    let pdta = list(
        b"pdta",
        &[
            chunk(b"phdr", &phdr),
            chunk(b"pbag", &pbag),
            chunk(b"pmod", &pmod),
            chunk(b"pgen", &pgen),
            chunk(b"inst", &inst),
            chunk(b"ibag", &ibag),
            chunk(b"imod", &imod),
            chunk(b"igen", &igen),
            chunk(b"shdr", &shdr),
        ],
    );
    let mut body = b"sfbk".to_vec();
    body.extend(info);
    body.extend(sdta);
    body.extend(pdta);
    chunk(b"RIFF", &body)
}
