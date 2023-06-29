use std::env;
use std::fs::read;
use std::fs::File;
use std::io::prelude::*;
use std::io::Write;
use std::io::{self, BufRead};
use std::path::Path;
use std::process;

struct Dx {
    dx_fbuf: [u8; 8],
    lp_scale: [i8; 4],
}

struct Work {
    dx: Dx,
}

struct D {
    headlen: u32,
    datalen: u32,
    loop_len: u32,
    work: Work,
}

static DVI_DLT_TBL: [u16; 89] = [
    0x000c, 0x0010, 0x0012, 0x0014, 0x0015, 0x0018, 0x0019, 0x001b, 0x0020, 0x0022, 0x0025, 0x0029,
    0x002c, 0x0032, 0x0038, 0x003c, 0x0044, 0x0049, 0x0052, 0x0059, 0x0064, 0x006c, 0x0078, 0x0084,
    0x0092, 0x00a0, 0x00b0, 0x00c2, 0x00d5, 0x00eb, 0x0104, 0x011c, 0x0139, 0x0159, 0x017b, 0x01a2,
    0x01cb, 0x01f9, 0x022c, 0x0265, 0x02a2, 0x02e5, 0x0330, 0x0382, 0x03db, 0x0440, 0x04ab, 0x0524,
    0x05a8, 0x0638, 0x06d8, 0x0785, 0x0848, 0x091b, 0x0a04, 0x0b05, 0x0c20, 0x0d55, 0x0eab, 0x1024,
    0x11c0, 0x1385, 0x1579, 0x17a0, 0x19fc, 0x1c98, 0x1f74, 0x2298, 0x260c, 0x29db, 0x2e0b, 0x32a8,
    0x37b8, 0x3d49, 0x436b, 0x4a29, 0x5194, 0x59bc, 0x62b5, 0x6c95, 0x7772, 0x8364, 0x9088, 0x9efb,
    0xaee2, 0xc05c, 0xd39b, 0xe8c4, 0xfffc,
];

static OKI_SCALE_TBL: [i8; 8] = [-1, -1, -1, -1, 2, 4, 6, 8];

struct LibPcmDecoder {
    srate: u32,
    datalen: u32,
    slen: u32,
    bitrate: u32,
    loop_len: u32,
    cur: u32,
    cur2: u32,
    readbuf_p: u32,
    readbuf_l: u32,
    adp_delta: [i32; 2],
    ba: usize,
    nch: usize,
    bps: usize,
    headlen: usize,
    loop_enable: usize,
    adp_scale: [i8; 2],
    bit_buf: u8,
    bit_pos: u8,
    codec: u8,
    work: Work,
}

const LIBPCM_INVALID_SAMPLE_VALUE: u32 = 0x66666666;

static mut numblocks: i32 = 0;

fn get_sample_draculax(
    d: &mut LibPcmDecoder,
    ch: usize,
    file: &Vec<u8>,
    file_pos: &mut usize
) -> i32 {
    let mut idx: u32;
    let mut delta: i32;
    let mut v;
    if d.bit_pos == 0 {
        for idx in 0..8 {
            v = file[*file_pos];
            *file_pos += 1;
            d.work.dx.dx_fbuf[idx] = v;
        }
        unsafe {
            numblocks += 1;
        }
    }
    if (d.bit_pos & 2) == 0 {
        v = (d.work.dx.dx_fbuf[(ch << 2) + (d.bit_pos >> 2) as usize] >> 4) & 0xf;
    } else {
        v = (d.work.dx.dx_fbuf[(ch << 2) + (d.bit_pos >> 2) as usize] >> 0) & 0xf;
    }
    d.bit_pos += 1;
    if d.bit_pos == 4 * 2 * 2 {
        d.bit_pos = 0;
    }
    let scale_index = d.adp_scale[ch] as usize;
    let dlt_value = DVI_DLT_TBL[scale_index] as u32;
    let v_masked = v as u16 & 7;
    let shifted_and_added = (v_masked << 1) + 1;
    let multiplied = (dlt_value * (shifted_and_added as u32)) as i32;
    let shifted = ((multiplied as u32) >> 4) as u16 as i32;
    delta = shifted;

    d.adp_scale[ch] += OKI_SCALE_TBL[(v & 7) as usize];
    if d.adp_scale[ch] > 88 {
        d.adp_scale[ch] = 88;
    } else if d.adp_scale[ch] < 0 {
        d.adp_scale[ch] = 0;
    }
    d.adp_delta[ch] += if (v & 8) != 0 {
        ((!(delta)).wrapping_add(1))
    } else {
        delta
    };
    if d.adp_delta[ch] > 0x7fff {
        d.adp_delta[ch] = 0x7fff;
    } else if d.adp_delta[ch] < -0x8000 {
        d.adp_delta[ch] = -0x8000;
    }
    d.adp_delta[ch]
}

fn libpcm_read_draculax(
    d: &mut LibPcmDecoder,
    buf: &mut [u8],
    nsamples: usize,
    the_file: &Vec<u8>,
    file_pos: &mut usize
) -> usize {
    let mut ret = 0;

    let mut ch = 0;
    let mut v;
    let mut buf_pos = 0;
    while ret < nsamples {
        if file_pos == &the_file.len() {
            break;
        }

        v = get_sample_draculax(d, ch, &the_file, file_pos);
        if v == LIBPCM_INVALID_SAMPLE_VALUE as i32 {
            break;
        }

        // each sample is 2 bytes.
        buf[buf_pos + 0] = (v >> (8 * 0)) as u8;
        buf[buf_pos + 1] = (v >> (8 * 1)) as u8;

        buf_pos += 2;
        ch += 1;
        if ch == d.nch {
            ch = 0;
            ret += 1;
            // loop
        }
    }
    ret
}

fn u8_to_u16_vec(input: Vec<u8>, little_endian: bool) -> Vec<i16> {
    input
        .chunks_exact(2)
        .map(|chunk| {
            if little_endian {
                (chunk[0] as i16) | ((chunk[1] as i16) << 8)
            } else {
                ((chunk[0] as i16) << 8) | chunk[1] as i16
            }
        })
        .collect()
}

#[derive(Debug)]
struct WavHeader {
    riff: [u8; 4],
    chunk_size: u32,
    wave: [u8; 4],
    fmt: [u8; 4],
    subchunk1_size: u32,
    audio_format: u16,
    num_of_chan: u16,
    samples_per_sec: u32,
    bytes_per_sec: u32,
    block_align: u16,
    bits_per_sample: u16,
    subchunk2_id: [u8; 4],
    subchunk2_size: u32,
}

fn main() -> io::Result<()> {
    let data_pos = 0;

    let args: Vec<String> = env::args().collect();

    if args.len() < 3 {
        println!("Please provide a file path.");
        process::exit(1);
    }

    let path = &args[1];
    let out_path = &args[2];

    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(_) => {
            println!("Failed to open file: {}", path);
            process::exit(1);
        }
    };

    let mut contents = Vec::new();
    match file.read_to_end(&mut contents) {
        Ok(_) => println!("Successfully read file."),
        Err(_) => {
            println!("Failed to read file: {}", path);
            process::exit(1);
        }
    }

    let mut d = LibPcmDecoder {
        srate: 0,
        datalen: 0,
        slen: 0,
        bitrate: 0,
        loop_len: 0,
        cur: 0,
        cur2: 0,
        readbuf_p: 0,
        readbuf_l: 0,
        adp_delta: [0; 2],
        ba: 0,
        nch: 0,
        bps: 0,
        headlen: 0,
        loop_enable: 0,
        adp_scale: [0; 2],
        bit_buf: 0,
        bit_pos: 0,
        codec: 0,
        work: Work {
            dx: Dx {
                dx_fbuf: [0; 8],
                lp_scale: [0; 4],
            },
        },
    };

    let size = contents.len();

    if size < 16 {
        println!("File is too short.");
        process::exit(1);
    }

    let header_u32 = u32::from_be_bytes([contents[0], contents[1], contents[2], contents[3]]);
    println!("Header {:08X}", header_u32);
    if header_u32 == 0x4456492E && size >= 0x800 {
        d.headlen =
            u32::from_be_bytes([contents[4], contents[5], contents[6], contents[7]]) as usize;
        println!("Header Length {:08X}", d.headlen);

        d.datalen = u32::from_be_bytes([contents[8], contents[9], contents[10], contents[11]]) << 2;
        println!("Data Length {:08X} {}", d.datalen, d.datalen);

        d.loop_len = u32::from_be_bytes([contents[12], contents[13], contents[14], contents[15]]);
        println!("Loop Length {:08X}", d.loop_len);

        d.work.dx.lp_scale[0] = contents[0x17] as i8;
        println!("lp_scale[0] {:02X}", d.work.dx.lp_scale[0]);

        d.work.dx.lp_scale[1] = contents[0x27] as i8;
        println!("lp_scale[1] {:02X}", d.work.dx.lp_scale[1]);

        if d.loop_len == 0xFFFFFFFF {
            d.loop_len = 0;
        } else {
            d.loop_len = u32::from_be_bytes([contents[8], contents[9], contents[10], contents[11]])
                - d.loop_len;
        }

        d.nch = 2;
        d.ba = 2 * 2;
        d.bps = 16;
        d.srate = 44100;
        println!("Adjusted loop length {:08X}", d.loop_len);

        let mut pos: usize = d.headlen;
        let nsamples: usize = d.datalen as usize / 2;

        let mut buf = vec![0; nsamples * 2];
        let mut numbers_pos = 0;
        libpcm_read_draculax(
            &mut d,
            &mut buf,
            nsamples,
            &contents,
            &mut pos
        );

        // write wav file
        let mut output_file = File::create(out_path)?;

        // Create wav header
        let mut wav_header = WavHeader {
            riff: *b"RIFF",
            chunk_size: 36,
            wave: *b"WAVE",
            fmt: *b"fmt ",
            subchunk1_size: 16,
            audio_format: 1,
            num_of_chan: 2,
            samples_per_sec: 44100,
            bytes_per_sec: 176400,
            block_align: 4,
            bits_per_sample: 16,
            subchunk2_id: *b"data",
            subchunk2_size: 0,
        };

        wav_header.bytes_per_sec = wav_header.samples_per_sec
            * wav_header.num_of_chan as u32
            * wav_header.bits_per_sample as u32
            / 8;
        wav_header.block_align = wav_header.num_of_chan * wav_header.bits_per_sample / 8;
        let temp = (d.datalen as usize / d.ba
            * wav_header.samples_per_sec as usize
            * wav_header.num_of_chan as usize
            * wav_header.bits_per_sample as usize)
            / 8;
        wav_header.subchunk2_size = temp as u32;
        wav_header.chunk_size = 36 + wav_header.subchunk2_size;

        // Write wav header to file
        output_file.write(&wav_header.riff)?;
        output_file.write(&wav_header.chunk_size.to_le_bytes())?;
        output_file.write(&wav_header.wave)?;
        output_file.write(&wav_header.fmt)?;
        output_file.write(&wav_header.subchunk1_size.to_le_bytes())?;
        output_file.write(&wav_header.audio_format.to_le_bytes())?;
        output_file.write(&wav_header.num_of_chan.to_le_bytes())?;
        output_file.write(&wav_header.samples_per_sec.to_le_bytes())?;
        output_file.write(&wav_header.bytes_per_sec.to_le_bytes())?;
        output_file.write(&wav_header.block_align.to_le_bytes())?;
        output_file.write(&wav_header.bits_per_sample.to_le_bytes())?;
        output_file.write(&wav_header.subchunk2_id)?;
        output_file.write(&wav_header.subchunk2_size.to_le_bytes())?;
        output_file.write_all(&buf).expect("Unable to write data");
    }
    Ok(())
}
