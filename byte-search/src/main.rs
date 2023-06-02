use std::fs::{self, File};
use std::io::{self, Read};
use std::path::Path;

fn search_byte_sequence_in_file(file_path: &Path, byte_sequence: &[u8]) -> io::Result<()> {
    let mut file = File::open(file_path)?;
    let chunk_size = 4096;
    let mut buffer = [0u8; 4096];
    let mut address = 0;

    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        let chunks = buffer[..bytes_read].windows(byte_sequence.len());

        for (index, chunk) in chunks.enumerate() {
            if chunk == byte_sequence {
                let byte_offset = address + index;
                println!(
                    "Byte sequence found at address {:#010x} in file: {:?}",
                    byte_offset, file_path
                );
            }
        }

        address += bytes_read;
    }

    Ok(())
}

fn search_byte_sequence_in_directory(dir_path: &Path, byte_sequence: &[u8]) -> io::Result<()> {
    let dir_entries = fs::read_dir(dir_path)?;

    for entry in dir_entries {
        let entry = entry?;
        let file_path = entry.path();

        if file_path.is_dir() {
            // Recursively search in subdirectories
            search_byte_sequence_in_directory(&file_path, byte_sequence)?;
        } else {
            search_byte_sequence_in_file(&file_path, byte_sequence)?;
        }
    }

    Ok(())
}

use std::env;
use std::error::Error;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        println!("Usage: {} <directory_path> <byte_sequence>", args[0]);
        return;
    }

    let dir_path = Path::new(&args[1]);
    let byte_sequence = parse_byte_sequence(&args[2]);

    match byte_sequence {
        Ok(byte_sequence) => {
            match search_byte_sequence_in_directory(&dir_path, &byte_sequence) {
                Ok(()) => println!("Search complete."),
                Err(err) => eprintln!("Error occurred: {}", err),
            }
        }
        Err(err) => eprintln!("Invalid byte sequence: {}", err),
    }
}

fn parse_byte_sequence(byte_sequence_str: &str) -> Result<Vec<u8>, Box<dyn Error>> {
    let byte_sequence = byte_sequence_str
        .split(',')
        .map(|byte_str| {
            let byte = u8::from_str_radix(byte_str.trim(), 16)?;
            Ok::<u8, Box<dyn Error>>(byte)
        })
        .collect::<Result<Vec<u8>, Box<dyn Error>>>()?;

    Ok(byte_sequence)
}
