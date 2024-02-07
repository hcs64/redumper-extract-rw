use std::env;
use std::fs;
use std::fs::File;
use std::io::{BufWriter, Write};
//use std::collections::HashMap;
//use std::collections::hash_map::Entry;

// deinterleave offsets, from karaoke-dx by way of cdgparse
const INTERLEAVE_OFFSET: [usize; 24] = [
    0, 66, 125, 191, 100, 50, 150, 175, 8, 33, 58, 83, 108, 133, 158, 183, 16, 41, 25, 91, 116,
    141, 166, 75,
];

const SECTOR_SIZE: usize = 96;
const PACKET_SIZE: usize = 24;
const SECTOR_SPREAD: usize = 2;
const LEADIN_SKIP_SECTORS: usize = (10 * 60 + 2) * 75;
const RW_MASK_8: u8 = 0x3f;
const RW_MASK_16: u16 = 0x3f3f;
//const RW_MASK_32: u32 = 0x3f3f3f3f;
//const RW_MASK_128: u128 = 0x3f3f3f3f_3f3f3f3f_3f3f3f3f_3f3f3f3f;

static EMPTY_PACKET: [u8; PACKET_SIZE] = [0; PACKET_SIZE];

fn gather_and_mask(buf: &[u8]) -> [u8; SECTOR_SIZE] {
    std::array::from_fn(|i| {
        let packet = i / PACKET_SIZE;
        let column = i % PACKET_SIZE;
        let b = buf[packet * PACKET_SIZE + INTERLEAVE_OFFSET[column]];
        b
    })
}

fn format_time(sector: usize) -> String {
    let frames = sector % 75;
    let seconds = sector / 75;
    let minutes = seconds / 60;
    let seconds = seconds % 60;

    format!("{minutes:02}:{seconds:02}.{frames:02}")
}

fn lookup_parity_p(packet: &[u8]) -> Option<u16> {
    let ty = packet[0] & RW_MASK_8;
    assert_eq!(ty, 0x09, "only CD+G type 9 commands supported");
    let command = packet[1] & RW_MASK_8;

    // this is a guess, these seem to be overwhelmingly common on VINL-1
    Some(match command {
        0x01 => 0x3c34,
        0x02 => 0x3932,
        0x06 => 0x353a,
        0x18 => 0x1706,
        0x1e => 0x1d0a,
        0x1f => 0x1e08,
        0x26 => 0x1639,
        _ => return None,
    })
}

fn main() {
    let args: Vec<_> = env::args_os().collect();
    let infile = fs::read(&args[1]).expect("read input");
    assert!(infile.len() % SECTOR_SIZE == 0);
    let mut outfile = BufWriter::new(File::create(&args[2]).expect("open output"));

    //let mut payload_map = std::collections::HashMap::new();
    //let mut p_map = std::collections::HashMap::new();
    //let mut q_map = std::collections::HashMap::new();
    for sector in LEADIN_SKIP_SECTORS..(infile.len() / SECTOR_SIZE).saturating_sub(SECTOR_SPREAD) {
        let relative_sector = sector - LEADIN_SKIP_SECTORS;
        let in_packets = gather_and_mask(
            &infile[sector * SECTOR_SIZE..(sector + SECTOR_SPREAD + 1) * SECTOR_SIZE],
        );

        for (packet_i, packet) in in_packets.chunks(PACKET_SIZE).enumerate() {
            let out_packet = match packet[0] & RW_MASK_8 {
                9 => {
                    //let payload = [&packet[0..2], &packet[4..20]].concat(); // parityQ might use this or
                    // 2..20?
                    //let data_p_channel = data.iter().enumerate().fold(0u16, |acc, (i, b)| acc | (u16::from((b >> 7) & 1) << i));
                    //let packet_p_channel = packet.iter().enumerate().fold(0u32, |acc, (i, b)| acc | (u32::from((b >> 7) & 1) << i));
                    //let packet_q_channel = packet.iter().enumerate().fold(0u32, |acc, (i, b)| acc | (u32::from((b >> 6) & 1) << i));
                    //let parity_q = u32::from_be_bytes(packet[20..24].try_into().unwrap()) & RW_MASK_32;

                    let parity_p = u16::from_be_bytes(packet[2..4].try_into().unwrap());
                    let expected_parity_p = lookup_parity_p(packet);

                    if expected_parity_p == Some(parity_p & RW_MASK_16) {
                        packet
                    } else if let Some(expected) = expected_parity_p {
                        println!(
                            "{time}: drop {relative_sector:6}.{packet_i}: tc {tc:04x} parityP {parity_p:04x} != {expected:04x}",
                            time = format_time(relative_sector),
                            tc = u16::from_be_bytes(packet[0..2].try_into().unwrap()),
                        );

                        &EMPTY_PACKET
                    } else {
                        println!(
                            "{time}: drop {relative_sector:6}.{packet_i}: tc {tc:04x} unknown command",
                            time = format_time(relative_sector),
                            tc = u16::from_be_bytes(packet[0..2].try_into().unwrap()),
                        );
                        &EMPTY_PACKET
                    }
                }
                _ => &EMPTY_PACKET, // strip other command types
            };
            outfile.write(out_packet).expect("write output");
        }
    }

    /*for (k, (count, v)) in type_command_map {
        if count > 1 {
            println!("{k:04x} {v:04x} ({count})");
        }
    }*/

    outfile.flush().expect("flush output");
}
