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

fn main() {
    let args: Vec<_> = env::args_os().collect();
    let infile = fs::read(&args[1]).expect("read input");
    assert!(infile.len() % SECTOR_SIZE == 0);
    let mut outfile = BufWriter::new(File::create(&args[2]).expect("open output"));

    let mut pack_count = 0;
    let mut p_corrected = 0;
    let mut p_uncorrected = 0;
    let mut q_error_count = 0;

    let mut zero_count = 0;
    let mut graphics_count = 0;
    let mut other_count = 0;

    for sector in LEADIN_SKIP_SECTORS..(infile.len() / SECTOR_SIZE).saturating_sub(SECTOR_SPREAD) {
        let relative_sector = sector - LEADIN_SKIP_SECTORS;
        let mut packet = gather_and_mask(
            &infile[sector * SECTOR_SIZE..(sector + SECTOR_SPREAD + 1) * SECTOR_SIZE],
        );

        for (pack_i, pack) in packet.chunks_mut(PACK_SIZE).enumerate() {
            pack_count += 1;

            if !p_parity::is_correct(pack) {
                if let Ok(_correct_errors) = p_parity::correct_errors(pack) {
                    p_corrected += 1;
                } else {
                    p_uncorrected += 1;

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
                q_error_count += 1;
                eprintln!(
                    "{time}: Q error {relative_sector:6}.{pack_i}: {tc:08x}",
                    time = format_time(relative_sector),
                    tc = u32::from_be_bytes(pack[0..4].try_into().unwrap()),
                );
            }

            match pack[0] >> 3 {
                0 => zero_count += 1,
                1 => graphics_count += 1,
                _ => other_count += 1,
            }

            outfile.write_all(pack).expect("write output");
        }
    }

    eprintln!("{pack_count:8} packs");
    eprintln!("P errors: {p_corrected:8} corrected / {p_uncorrected} uncorrected");
    eprintln!("Q errors: {q_error_count:8}");
    eprintln!();
    eprintln!("{zero_count:8} zero");
    eprintln!("{graphics_count:8} graphics");
    eprintln!("{other_count:8} other");

    outfile.flush().expect("flush output");
}
