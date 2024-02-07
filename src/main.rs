use std::env;
use std::fs;
use std::fs::File;
use std::io::{BufWriter, Write};

// deinterleave offsets, from karaoke-dx by way of cdgparse
const INTERLEAVE_OFFSET: [usize; 24] = [
    0, 66, 125, 191, 100, 50, 150, 175, 8, 33, 58, 83, 108, 133, 158, 183, 16, 41, 25, 91, 116,
    141, 166, 75,
];

const SECTOR_SIZE: usize = 96;
const SECTOR_SPREAD: usize = 2;
const LEADIN_SKIP_SECTORS: usize = (10 * 60 + 2) * 75;
const RW_MASK: u8 = 0x3f;

fn process(buf: &[u8]) -> [u8; SECTOR_SIZE] {
    std::array::from_fn(|i| {
        let pack = i / 24;
        let column = i % 24;
        let b = buf[pack * 24 + INTERLEAVE_OFFSET[column]];
        // mask to keep only subchannels R-W
        b & RW_MASK
    })
}

fn main() {
    let args: Vec<_> = env::args_os().collect();
    let infile = fs::read(&args[1]).expect("read input");
    assert!(infile.len() % SECTOR_SIZE == 0);
    let mut outfile = BufWriter::new(File::create(&args[2]).expect("open output"));

    for sector in LEADIN_SKIP_SECTORS..(infile.len() / SECTOR_SIZE).saturating_sub(SECTOR_SPREAD) {
        outfile
            .write(&process(
                &infile[sector * SECTOR_SIZE..(sector + SECTOR_SPREAD + 1) * SECTOR_SIZE],
            ))
            .expect("write output");
    }

    outfile.flush().expect("flush output");
}
