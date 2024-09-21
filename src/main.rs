use std::env;
use std::fs;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use gf256::{gf, rs::rs};

#[gf(polynomial = 0x43, generator = 0x2)]
type gf64;

#[rs(gf=gf64, u=u8, block=24, data=20)]
mod p_parity {}

#[rs(gf=gf64, u=u8, block=4, data=2)]
mod q_parity {}

const SECTOR_SIZE: usize = 96;
const PACK_SIZE: usize = 24;
const SECTOR_SPREAD: usize = 2;
const LEADIN_SKIP_SECTORS: usize = (10 * 60 + 2) * 75;
const RW_MASK: u8 = 0x3f;

const fn compute_deinterleave() -> [usize; SECTOR_SIZE] {
    let mut offsets = [0; SECTOR_SIZE];
    let mut i = 0;

    while i < SECTOR_SIZE {
        let pack = i / PACK_SIZE;
        let col = i % PACK_SIZE;
        let col = match col {
            1 => 18,
            18 => 1,

            2 => 5,
            5 => 2,

            3 => 23,
            23 => 3,
            _ => col,
        };
        let lookahead = col % 8;
        offsets[i] = (pack + lookahead) * PACK_SIZE + col;
        i += 1;
    }

    offsets
}

const DEINTERLEAVE: [usize; SECTOR_SIZE] = compute_deinterleave();

fn deinterleave_and_mask(buf: &[u8]) -> [u8; SECTOR_SIZE] {
    std::array::from_fn(|i| {
        let b = buf[DEINTERLEAVE[i]];
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
        eprintln!("  total:    {:8}", self.total);
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

struct CorrectResult {
    p_corrected: bool,
    p_uncorrected: bool,
    q_error: bool,
}

fn correct_pack(pack: &mut [u8]) -> CorrectResult {
    assert_eq!(pack.len(), PACK_SIZE);

    let mut result = CorrectResult {
        p_corrected: false,
        p_uncorrected: false,
        q_error: false,
    };

    if !p_parity::is_correct(pack) {
        if let Ok(_correct_errors) = p_parity::correct_errors(pack) {
            result.p_corrected = true;
        } else {
            result.p_uncorrected = true;
        }
    }

    // FIXME: I don't know if it's worth trying to fix Q, and if
    // so whether to attempt it before P, or maybe only if P fails.
    if !q_parity::is_correct(&pack[0..4]) {
        result.q_error = true;
    }

    result
}

fn parse_end_lba(toc: &[u8]) -> usize {
    // Parse last track LBA from SCSI MMC TOC Descriptor
    fn ra<const N: usize>(b: &[u8], offset: usize) -> [u8; N] {
        b[offset..offset + N].try_into().unwrap()
    }
    let data_length: usize = u16::from_be_bytes(ra(toc, 0)).into();
    assert_eq!(data_length + 2, toc.len());
    const TOC_DESCRIPTOR_SIZE: usize = 8;
    let track_count = (data_length - 2) / TOC_DESCRIPTOR_SIZE;
    assert!(track_count > 0);
    assert_eq!(track_count * TOC_DESCRIPTOR_SIZE + 4, toc.len());

    let first_descriptor_offset = 4;
    let track_aa_offset = first_descriptor_offset + (track_count - 1) * TOC_DESCRIPTOR_SIZE;
    let track_aa_number = toc[track_aa_offset + 2];
    assert_eq!(track_aa_number, 0xaa);
    u32::from_be_bytes(ra(toc, track_aa_offset + 4))
        .try_into()
        .unwrap()
}

fn main() -> ExitCode {
    let args: Vec<_> = env::args_os().collect();
    let image_name = &args[1];
    let mut infile_name = PathBuf::from(image_name);
    infile_name.set_extension("subcode");
    let infile = match fs::read(&infile_name) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("failed reading input .subcode {infile_name:?}: {e}");
            return ExitCode::FAILURE;
        }
    };
    assert!(infile.len() % SECTOR_SIZE == 0);
    let infile_sectors = infile.len() / SECTOR_SIZE;
    infile_name.set_extension("toc");
    let end_lba = match fs::read(&infile_name) {
        Ok(infile_toc) => {
            parse_end_lba(&infile_toc)
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            eprintln!("input .toc not found, using whole .subcode length");
            infile_sectors
        }
        Err(e) => {
            eprintln!("failed reading input .toc {infile_name:?}: {e}");
            return ExitCode::FAILURE;
        }
    };
    // parse toc
    let end_toc_absolute_sector = LEADIN_SKIP_SECTORS + end_lba;
    eprintln!(
        "parsing [{}..{}), total {} sectors",
        LEADIN_SKIP_SECTORS,
        end_toc_absolute_sector,
        end_toc_absolute_sector - LEADIN_SKIP_SECTORS
    );

    let mut outfile = BufWriter::new(File::create(&args[2]).expect("open output"));

    let mut all_count = PackCount::new("All");
    let mut zero_count = PackCount::new("zero");
    let mut line_graphics_count = PackCount::new("line graphics");
    let mut cdg_count = PackCount::new("CD+G");
    let mut cdeg_count = PackCount::new("CD+EG");
    let mut other_graphics_count = PackCount::new("other graphics");
    let mut other_count = PackCount::new("other");

    let mut oddity = false;

    eprintln!("----");

    for sector in 0..infile_sectors {
        if sector < LEADIN_SKIP_SECTORS || sector >= end_toc_absolute_sector {
            let relative_sector = (sector as isize) - (LEADIN_SKIP_SECTORS as isize);
            if sector < SECTOR_SPREAD {
                if !infile[sector * SECTOR_SIZE..(sector + 1) * SECTOR_SIZE]
                    .iter()
                    .all(|b| *b == 0)
                {
                    oddity = true;
                    eprintln!("non-zero sector at start of .subcode: {relative_sector}");
                }
            } else {
                // Just check for non-zero packs outside of the TOC range
                let mut packet = deinterleave_and_mask(
                    &infile[(sector - SECTOR_SPREAD) * SECTOR_SIZE..(sector + 1) * SECTOR_SIZE],
                );
                for (pack_i, pack) in packet.chunks_mut(PACK_SIZE).enumerate() {
                    let result = correct_pack(pack);
                    if result.q_error || result.p_uncorrected || (pack[0] >> 3 != 0) {
                        oddity = true;
                        eprintln!("non-zero pack outside TOC at {relative_sector:6}.{pack_i}");
                    }
                }
            }

            continue;
        }

        let relative_sector = sector - LEADIN_SKIP_SECTORS;
        // Deinterleave, looking back two sectors for delayed symbols
        let mut packet = deinterleave_and_mask(
            &infile[(sector - SECTOR_SPREAD) * SECTOR_SIZE..(sector + 1) * SECTOR_SIZE],
        );

        for (pack_i, pack) in packet.chunks_mut(PACK_SIZE).enumerate() {
            let result = correct_pack(pack);
            if result.p_uncorrected {
                //let mut expected = pack.to_owned();
                //p_parity::encode(&mut expected);
                oddity = true;
                eprintln!(
                    "{time}: P uncorrected {relative_sector:6}.{pack_i}",
                    time = format_time(relative_sector),
                );
            }

            if result.q_error {
                oddity = true;
                eprintln!(
                    "{time}: Q error {relative_sector:6}.{pack_i}: {tc:08x}",
                    time = format_time(relative_sector),
                    tc = u32::from_be_bytes(pack[0..4].try_into().unwrap()),
                );
            }

            let pack_type_count = if result.q_error {
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
            if result.p_corrected {
                all_count.p_corrected += 1;
                pack_type_count.p_corrected += 1;
            }
            if result.p_uncorrected {
                all_count.p_uncorrected += 1;
                pack_type_count.p_uncorrected += 1;
            }
            if result.q_error {
                all_count.q_error += 1;
                pack_type_count.q_error += 1;
            }

            outfile.write_all(pack).expect("write output");
        }
    }

    for sector in infile_sectors.checked_sub(2).unwrap()..infile_sectors {
        if !infile[sector * SECTOR_SIZE..(sector + 1) * SECTOR_SIZE]
            .iter()
            .all(|b| *b & RW_MASK == 0)
        {
            let relative_sector = (sector as isize) - (LEADIN_SKIP_SECTORS as isize);
            oddity = true;
            eprintln!("non-zero RW in sector at end of .subcode: {relative_sector}");
        }
    }

    if !oddity {
        eprintln!("OK!");
    }
    eprintln!("----");

    all_count.report();
    zero_count.report();
    line_graphics_count.report();
    cdg_count.report();
    cdeg_count.report();
    other_graphics_count.report();
    other_count.report();

    outfile.flush().expect("flush output");

    ExitCode::SUCCESS
}
