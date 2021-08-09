use std::process::exit;
use anyhow::{Context, Result};
use clap::{App, Arg, crate_version};
use std::fs;
use std::io::Read;
use serde::Deserialize;
use once_cell::sync::OnceCell;
use chrono::Local;

// グローバルな設定情報オブジェクト
pub static CONFIG: OnceCell<Config> = OnceCell::new();



#[derive(Debug,Clone,Deserialize)]
pub struct Config{
    pub initial_college_csv: String,
    pub student_number: usize,
    pub random_seed: u64,
    pub output_dir: String,

    pub college_rank_lower: [i32; 3],
    pub college_rank_upper: [i32; 3],
    pub college_rank_select_number: [usize; 3],
}

impl Config {
    ///////////////////////////////////////////////////////
    // 定数定義
    //大学入試３マトリクスの各生成値の意味
    pub const APPLY: u8 = 1;   //受験（学生）
    pub const ENROLL: u8 = 2;  //合格判定（大学）
    pub const ADMISSION: u8 = 4; //入学先に決定（学生）

    //大学入試結果resultマトリクス集計時の意味
    pub const R_FAILED: u8 = 1;  //不合格
    pub const R_PASSED: u8 = 3;  //合格
    pub const R_ADMISSION: u8 = 7; //入学

    //大学設定区分
    pub const NATIONAL: u8 = 1; //国立
    pub const PUBLIC: u8 = 2; //公立
    pub const PRIVATE: u8 = 3; //私立

    //入学定員超過率の年度別上限
    pub const MAX_ENROLLMENT_RATES: [[f64; 3]; 4] = [
        // 大学規模L M S
        [1.20, 1.30, 1.30], // < 2016
        [1.17, 1.27, 1.30], //  2016
        [1.14, 1.24, 1.30], //  2017
        [1.10, 1.20, 1.30], //  2018 ~
    ];

    ///////////////////////////////////////////////////////
    // ここから関数定義
    // Configオブジェクト生成。　コマンドライン引数の設定ファイルから。
    // Configオブジェクトは一度だけstaticで生成され、その後不変。
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

    // Configオブジェクト生成。　関数引数に直接指定された設定ファイル名から。
    pub fn from_path(path: &str) -> Result<()>{
        let mut f = fs::File::open(path).expect("config toml file not found");
        println!("設定ファイルは{:?}です。", path);
        let mut contents = String::new();
        f.read_to_string(&mut contents).expect("config file read error");
        let cfg: Config = toml::from_str(&contents).unwrap();
        CONFIG.set(cfg).unwrap();
        Ok(())
    }

    // 生成済みのConfigオブジェクトを返す
    pub fn get() -> &'static Config{
        CONFIG.get().expect("Not initalized Config")
    }

    //データ出力ディレクトリを生成し、その相対パス名を返す。
    pub fn get_output_dirname() -> Result<String>{
        let new_dir = Config::get().output_dir.clone() + "/" + &Local::now().format("%Y%m%d%H%M%S").to_string();
        match fs::create_dir(new_dir.clone()).context("dir cannot create"){
            Err(e) => Err(e),
            Ok(_) => Ok(new_dir + "/"),
        }
    }
}