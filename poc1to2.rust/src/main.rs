extern crate clap;
use clap::{Arg, App};
use std::io::prelude::*;
use std::path::{Path, PathBuf};
use std::fs;
use std::fs::File;
use std::fs::OpenOptions;
use std::io::SeekFrom;

const SCOOPS_IN_NONCE: i64 = 4096;
const SHABAL256_HASH_SIZE: i64 = 32;
const SCOOP_SIZE: i64 = SHABAL256_HASH_SIZE * 2;
const NONCE_SIZE: i64 = SCOOP_SIZE * SCOOPS_IN_NONCE;

struct Plot<'a> {
    id: u64,
    offset: u64,
    nonces: i64,
    size: u64,
    path: &'a Path,
    out_dir: &'a Path,
    inline: bool,
}

impl<'a> Plot<'a> {
    fn new(path: &'a str, out: Option<&'a str>) -> Plot<'a> {
        let path = Path::new(path);
        if !path.exists() {
            panic!("plot path does not exists");
        };
        if !path.is_file() {
            panic!("plot path is not a file");
        };

        let (out_dir, inline) = if out.is_some() {
            let out_dir = Path::new(out.unwrap());
            if !out_dir.is_dir() {
                panic!("{} is not not a directory", out.unwrap());
            }
            (out_dir, false)
        } else {
            (path.parent().unwrap(), true)
        };

        let plot_file = path.file_name().unwrap().to_str().unwrap();
        let parts: Vec<&str> = plot_file.split("_").collect();
        if parts.len() < 4 {
            panic!("plot file has wrong format")
        }

        let id_res = parts[0].parse::<u64>();
        if id_res.is_err() {
            panic!("id of plotfile has wrong format")
        }

        let offset_res = parts[1].parse::<u64>();
        if offset_res.is_err() {
            panic!("offset of plotfile has wrong format")
        }

        let nonces_res = parts[2].parse::<i64>();
        if nonces_res.is_err() {
            panic!("nonces of plotfile has wrong format")
        }

        let stagger_res = parts[3].parse::<i64>();
        if stagger_res.is_err() {
            panic!("stagger of plotfile has wrong format")
        }

        let nonces = nonces_res.unwrap();
        let stagger = stagger_res.unwrap();
        if nonces != stagger {
            panic!("converter only works with optimized plotfiles");
        };

        let size = fs::metadata(path).unwrap().len();
        let exp_size = nonces * NONCE_SIZE;
        if size != exp_size as u64 {
            panic!("expected plot size {} but got {}", exp_size, size);
        };


        Plot {
            id: id_res.unwrap(),
            offset: offset_res.unwrap(),
            nonces: nonces,
            size: size,
            path: path,
            out_dir: out_dir,
            inline: inline,
        }
    }

    fn convert(&self, quiet: bool) {
        let mut from = File::open(self.path).unwrap();
        let block_size = self.nonces * SCOOP_SIZE;
        let mut buffer1 = vec![0; block_size as usize];
        let mut buffer2 = vec![0; block_size as usize];

        let mut to = if self.inline {
            OpenOptions::new().write(true).open(self.path).unwrap()
        } else {
            let mut p = PathBuf::from(self.out_dir);
            p.push(self.poc2_name());
            let f = File::create(&p).unwrap();
            if f.set_len(self.size).is_err() {
                panic!("failed to preallocate size {}", self.size);
            };
            OpenOptions::new().write(true).open(p.as_path()).unwrap()
        };

        if !quiet { println!("start processing scoops"); };
        for scoop in 0i64 .. SCOOPS_IN_NONCE / 2 {
            let pos = scoop * block_size;

            from.seek(SeekFrom::Start(pos as u64)).unwrap();
            from.read_exact(&mut buffer1).unwrap();

            from.seek(SeekFrom::End(-pos - block_size)).unwrap();
            from.read_exact(&mut buffer2).unwrap();

            if !quiet {
	       print!("{}/{} ", scoop, SCOOPS_IN_NONCE - scoop);
	       std::io::stdout().flush().unwrap();
	    };

            let mut off: usize = 32;
            for _ in 0 .. self.nonces {
                let mut hash1 = [0;SHABAL256_HASH_SIZE as usize];
                hash1.copy_from_slice(&buffer1[off..off+SHABAL256_HASH_SIZE as usize]);
                buffer1[off..off+SHABAL256_HASH_SIZE as usize].copy_from_slice(
                    &buffer2[off..off+SHABAL256_HASH_SIZE as usize]);
                buffer2[off..off+SHABAL256_HASH_SIZE as usize].copy_from_slice(&hash1);
                off += SCOOP_SIZE as usize;
            }

            to.seek(SeekFrom::End(-pos - block_size)).unwrap();
            to.write_all(&buffer2).unwrap();

            to.seek(SeekFrom::Start(pos as u64)).unwrap();
            to.write_all(&buffer1).unwrap();
        }

        if self.inline {
            let out = PathBuf::from(self.out_dir).join(self.poc2_name());
            fs::rename(self.path, out).unwrap();
        };
        if !quiet { println!("finished processing scoops"); };
    }

    fn poc2_name(&self) -> String {
        self.id.to_string() + "_" + &self.offset.to_string() + "_" + &self.nonces.to_string()
    }
}

fn main() {
    let matches = App::new("PoC1 to PoC2 Converter")
        .version("0.0.1")
        .author("PoC Consortium <bots@cryptoguru.org>")
        .about("converts PoC1 plots to PoC2 plots")
        .arg(Arg::with_name("in")
             .required(true)
             .index(1))
        .arg(Arg::with_name("out")
             .short("o")
             .long("out")
             .help("Define a directory to write the converted plot file to. This switches
to copy on write mode. (Else in-place is default) and allows you to
fasten up the conversion at the expense of temporary additional HDD
space.")
             .takes_value(true))
        .arg(Arg::with_name("quiet")
             .short("q")
             .long("quiet")
             .help("Quiet operation. Really quiet - no output at all (except failures).
You can send the process into background and forget about it.")).get_matches();

    let quiet = matches.is_present("quiet");
    let plot = Plot::new(matches.value_of("in").unwrap(), matches.value_of("out"));
    plot.convert(quiet);
}

#[cfg(test)]
mod tests {
    extern crate md5;
    use super::*;

    #[test]
    fn test_plot() {
        let plot_file = "test_data/11253871103436815155_0_10_10";
        fs::copy(plot_file.to_owned() + ".orig", plot_file);

        let plot = Plot::new(plot_file, None);

        assert_eq!(plot.id, 11253871103436815155);
        assert_eq!(plot.offset, 0);
        assert_eq!(plot.nonces, 10);
        assert_eq!(plot.path, Path::new(plot_file));

        let poc2_plot_file = PathBuf::from(plot.out_dir).join(plot.poc2_name());
        assert_eq!(poc2_plot_file.to_str().unwrap(), "test_data/11253871103436815155_0_10");

        plot.convert(true);
        let mut buffer = Vec::new();
        File::open(&poc2_plot_file).unwrap().read_to_end(&mut buffer);

        let digest = md5::compute(buffer);
        assert_eq!(format!("{:x}", digest), "5dbd4aa4033b5877f37744a12c0f573c");

        fs::remove_file(&poc2_plot_file);
    }
}
