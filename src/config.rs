use std::process::exit;
use anyhow::Result;
use clap::{App, Arg, crate_version};
use std::fs;
use std::io::Read;
use serde::Deserialize;
use once_cell::sync::OnceCell;

pub static CONFIG: OnceCell<Config> = OnceCell::new();


#[derive(Debug,Clone,Deserialize)]
pub struct Config{
    pub initial_college_csv: String,
    pub student_number: usize,
    pub random_seed: u64,

    pub college_rank_lower: [i32; 3],
    pub college_rank_upper: [i32; 3],
    pub college_rank_select_number: [usize; 3],
}

impl Config {
    pub fn from_args() -> Result<()>{
        let matches = App::new("大学受験戦略シミュレーション")
            .version(crate_version!())
            .arg(Arg::with_name("CONFIG_FILE").help("設定ファイル名"))
            .get_matches();

        if let Some(filename) = matches.value_of("CONFIG_FILE") {
            let mut f = fs::File::open(filename).expect("config toml file not found");
            println!("設定ファイルは{:?}です。", filename);
            let mut contents = String::new();
            f.read_to_string(&mut contents).expect("config file read error");
            let cfg: Config = toml::from_str(&contents).unwrap();
            CONFIG.set(cfg).unwrap();
            Ok(())
        } else {
            println!("設定ファイル名が指定されていません。");
            exit(1)
        }
    }

    pub fn from_path(path: &str) -> Result<()>{
        let mut f = fs::File::open(path).expect("config toml file not found");
        println!("設定ファイルは{:?}です。", path);
        let mut contents = String::new();
        f.read_to_string(&mut contents).expect("config file read error");
        let cfg: Config = toml::from_str(&contents).unwrap();
        CONFIG.set(cfg).unwrap();
        Ok(())
    }

    pub fn get() -> &'static Config{
        CONFIG.get().expect("Not initalized Config")
    }
}

pub const MAX_ENROLLMENT_RATES: [[f64; 3]; 4] = [
    // 大学規模L M S
    [1.20, 1.30, 1.30], // < 2016
    [1.17, 1.27, 1.30], //  2016
    [1.14, 1.24, 1.30], //  2017
    [1.10, 1.20, 1.30], //  2018 ~
];