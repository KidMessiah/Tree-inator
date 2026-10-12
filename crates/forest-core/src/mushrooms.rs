//! Baked mushroom sprites (generated offline): five seeds of five kinds, seen from above.
//! They are ordinary tree sprites, so the engine shades, shadows and colours them like trees: the species ramp
//! colours the cap, and the species bark colour paints the warts.

use crate::sprite::Sprite;
use std::sync::OnceLock;

static DATA: &[u8] = include_bytes!("../assets/mushrooms.bin");
/// The same sprites baked at twice the size, for crowns bigger than the base sprite.
static DATA_2X: &[u8] = include_bytes!("../assets/mushrooms2x.bin");
static SPRITES: OnceLock<Vec<(String, u32, Sprite)>> = OnceLock::new();
static SPRITES_2X: OnceLock<Vec<(String, u32, Sprite)>> = OnceLock::new();

/// Mushroom kinds as (tree type name, label).
pub const KINDS: &[(&str, &str)] = &[
    ("mushroom_redcap", "Redcap"),
    ("mushroom_brown", "Brown cap"),
    ("mushroom_violet", "Violet cap"),
    ("mushroom_funnel", "Funnel cap"),
    ("mushroom_puffball", "Puffball"),
];

fn decode(data: &[u8]) -> (Vec<u8>, usize, usize) {
    let mut r = png::Decoder::new(std::io::Cursor::new(data)).read_info().expect("mushroom png");
    let mut buf = vec![0; r.output_buffer_size().expect("png size")];
    let info = r.next_frame(&mut buf).expect("mushroom frame");
    buf.truncate(info.buffer_size());
    (buf, info.width as usize, info.height as usize)
}

fn load(data: &'static [u8]) -> Vec<(String, u32, Sprite)> {
    let rd = |p: &mut usize, n: usize| -> &[u8] {
        let s = &data[*p..*p + n];
        *p += n;
        s
    };
    let mut p = 0;
    let count = u32::from_le_bytes(rd(&mut p, 4).try_into().unwrap());
    let mut out = Vec::new();
    for _ in 0..count {
        let nl = rd(&mut p, 1)[0] as usize;
        let name = String::from_utf8_lossy(rd(&mut p, nl)).to_string();
        let variant = rd(&mut p, 1)[0] as u32;
        let _size = u16::from_le_bytes(rd(&mut p, 2).try_into().unwrap());
        let _zmax = f32::from_le_bytes(rd(&mut p, 4).try_into().unwrap());
        let (la, lb) = (u32::from_le_bytes(rd(&mut p, 4).try_into().unwrap()) as usize, u32::from_le_bytes(rd(&mut p, 4).try_into().unwrap()) as usize);
        let (a, w, h) = decode(rd(&mut p, la));
        let (b, ..) = decode(rd(&mut p, lb));
        let n = w * h;
        let mut sp = Sprite { size: w, z: vec![0.0; n], nx: vec![0.0; n], ny: vec![0.0; n], tone: vec![0.0; n], depth: vec![0.0; n], cls: vec![0; n], zmax: 0.0 };
        for i in 0..n {
            let z16 = u16::from_be_bytes([b[2 * i], b[2 * i + 1]]);
            if z16 == 0 {
                continue;
            }
            let wart = z16 & 0x8000 != 0;
            sp.z[i] = (z16 & 0x7FFF) as f32 / 32767.0;
            sp.nx[i] = (a[4 * i] as f32 - 128.0) / 80.0;
            sp.ny[i] = (a[4 * i + 1] as f32 - 128.0) / 80.0;
            sp.tone[i] = a[4 * i + 2] as f32 / 255.0;
            sp.depth[i] = a[4 * i + 3] as f32 / 255.0;
            sp.cls[i] = if wart { 2 } else { 1 };
            sp.zmax = sp.zmax.max(sp.z[i]);
        }
        out.push((name, variant, sp));
    }
    out
}

/// The baked sprite for a mushroom type and seed, built for the crown radius `treegen_sprite::R0`, or twice
/// that with `big`.
pub fn sprite(name: &str, variant: u32, big: bool) -> Option<Sprite> {
    let all = if big { SPRITES_2X.get_or_init(|| load(DATA_2X)) } else { SPRITES.get_or_init(|| load(DATA)) };
    all.iter().find(|(n, v, _)| n == name && *v == variant % 5).map(|(_, _, s)| Sprite { size: s.size, z: s.z.clone(), nx: s.nx.clone(), ny: s.ny.clone(), tone: s.tone.clone(), depth: s.depth.clone(), cls: s.cls.clone(), zmax: s.zmax })
}

pub fn is_mushroom(name: &str) -> bool {
    name.starts_with("mushroom_")
}
