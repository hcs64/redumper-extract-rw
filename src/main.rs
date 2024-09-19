use std::env;
use std::fs;
use std::fs::File;
use std::io::{BufWriter, Write};

use gf256::{gf, rs::rs};

#[gf(polynomial = 0x43, generator = 0x2)]
type gf64;

#[rs(gf=gf64, u=u8, block=24, data=20)]
mod p_parity {}

#[rs(gf=gf64, u=u8, block=4, data=2)]
mod q_parity {}

// deinterleave offsets, from karaoke-dx by way of cdgparse
const INTERLEAVE_OFFSET: [usize; 24] = [
    0, 66, 125, 191, 100, 50, 150, 175, 8, 33, 58, 83, 108, 133, 158, 183, 16, 41, 25, 91, 116,
    141, 166, 75,
];

const SECTOR_SIZE: usize = 96;
const PACK_SIZE: usize = 24;
const SECTOR_SPREAD: usize = 2;
const LEADIN_SKIP_SECTORS: usize = (10 * 60 + 2) * 75;
const RW_MASK: u8 = 0x3f;

fn gather_and_mask(buf: &[u8]) -> [u8; SECTOR_SIZE] {
    std::array::from_fn(|i| {
        let packet = i / PACK_SIZE;
        let column = i % PACK_SIZE;
        let b = buf[packet * PACK_SIZE + INTERLEAVE_OFFSET[column]];
        b & RW_MASK
    })
}

fn format_time(sector: usize) -> String {
    let frames = sector % 75;
    let seconds = sector / 75;
    let minutes = seconds / 60;
    let seconds = seconds % 60;

    format!("{minutes:02}:{seconds:02}.{frames:02}")
}

struct PackCount {
    total: usize,
    p_corrected: usize,
    p_uncorrected: usize,
    q_error: usize,
    name: String,
}

impl PackCount {
    fn new(name: &str) -> Self {
        Self {
            total: 0,
            p_corrected: 0,
            p_uncorrected: 0,
            q_error: 0,
            name: name.to_owned(),
        }
    }

    fn report(&self) {
        if self.total == 0 {
            return;
        }
        eprintln!("{} packs:", self.name);
        eprintln!("  total: {:8}", self.total);
        if self.p_uncorrected > 0 {
            eprintln!(
                "  P errors: {:8} corrected / {} uncorrected",
                self.p_corrected, self.p_uncorrected
            );
        } else {
            eprintln!("  P errors: {:8} corrected", self.p_corrected);
        }
        if self.q_error > 0 {
            eprintln!("  Q errors: {:8}", self.q_error);
        }
        eprintln!();
    }
}

fn main() {
    let args: Vec<_> = env::args_os().collect();
    let infile = fs::read(&args[1]).expect("read input");
    assert!(infile.len() % SECTOR_SIZE == 0);
    let mut outfile = BufWriter::new(File::create(&args[2]).expect("open output"));

    let mut all_count = PackCount::new("All");
    let mut zero_count = PackCount::new("zero");
    let mut line_graphics_count = PackCount::new("line graphics");
    let mut cdg_count = PackCount::new("CD+G");
    let mut cdeg_count = PackCount::new("CD+EG");
    let mut other_graphics_count = PackCount::new("other graphics");
    let mut other_count = PackCount::new("other");

    for sector in LEADIN_SKIP_SECTORS..(infile.len() / SECTOR_SIZE).saturating_sub(SECTOR_SPREAD) {
        let relative_sector = sector - LEADIN_SKIP_SECTORS;
        let mut packet = gather_and_mask(
            &infile[sector * SECTOR_SIZE..(sector + SECTOR_SPREAD + 1) * SECTOR_SIZE],
        );

        for (pack_i, pack) in packet.chunks_mut(PACK_SIZE).enumerate() {
            let mut p_corrected = false;
            let mut p_uncorrected = false;
            let mut q_error = false;

            if !p_parity::is_correct(pack) {
                if let Ok(_correct_errors) = p_parity::correct_errors(pack) {
                    p_corrected = true;
                } else {
                    p_uncorrected = true;

                    let mut expected = pack.to_owned();
                    p_parity::encode(&mut expected);
                    eprintln!(
                        "{time}: P uncorrected {relative_sector:6}.{pack_i}",
                        time = format_time(relative_sector),
                    );
                }
            }

            // FIXME: I don't know if it's worth trying to fix Q, and if
            // so whether to attempt it before P, or maybe only if P fails.
            if !q_parity::is_correct(&pack[0..4]) {
                q_error = true;
                eprintln!(
                    "{time}: Q error {relative_sector:6}.{pack_i}: {tc:08x}",
                    time = format_time(relative_sector),
                    tc = u32::from_be_bytes(pack[0..4].try_into().unwrap()),
                );
            }

            let pack_type_count = if q_error {
                &mut other_count
            } else {
                match pack[0] >> 3 {
                    0 => &mut zero_count,
                    1 => match pack[0] & 0b111 {
                        0 => &mut line_graphics_count,
                        1 => &mut cdg_count,
                        2 => &mut cdeg_count,
                        _ => &mut other_graphics_count,
                    },
                    _ => &mut other_count,
                }
            };

            all_count.total += 1;
            pack_type_count.total += 1;
            if p_corrected {
                all_count.p_corrected += 1;
                pack_type_count.p_corrected += 1;
            }
            if p_uncorrected {
                all_count.p_uncorrected += 1;
                pack_type_count.p_uncorrected += 1;
            }
            if q_error {
                all_count.q_error += 1;
                pack_type_count.q_error += 1;
            }

            outfile.write_all(pack).expect("write output");
        }
    }

    all_count.report();
    zero_count.report();
    line_graphics_count.report();
    cdg_count.report();
    cdeg_count.report();
    other_graphics_count.report();
    other_count.report();

    outfile.flush().expect("flush output");
}
