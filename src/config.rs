use std::collections::HashMap;
use std::process::exit;
use anyhow::{Context, Result};
use clap::{App, Arg, crate_version};
use std::fs;
use std::io::Read;
use serde::Deserialize;
use once_cell::sync::OnceCell;
use chrono::Local;
use csv::ReaderBuilder;


use crate::college::EnrollAndCapa;

// グローバルな設定情報オブジェクト
pub static CONFIG: OnceCell<Config> = OnceCell::new();

#[derive(Debug,Clone,Deserialize)]
pub struct Config{
    pub initial_college_csv: String,
    pub student_number: Vec<usize>,
    pub student_dev_mu: f64,
    pub student_dev_sigma: f64,
    pub random_seed: u64,
    pub output_dir_base: String,
    pub output_dir: String,

    pub national_prob: f64,
    pub national_range: [i32; 2],
    pub college_rank_lower: [i32; 3],
    pub college_rank_upper: [i32; 3],
    pub college_rank_select_number: [[usize; 3]; 2],
    
    pub first_pattern_rate: f64,
    pub enroll_add_rate: f64,
    pub enroll_add_lower: i32,

    pub epochs: i32,
    pub start_year: usize,

    pub college_select_by_enroll: bool,

    pub small_college_support: bool,

    pub update_dev: bool,

    pub enroll_capa_csv_dir: String,
    pub enroll_capa_csv_name: String,
    pub enroll_capa_dics: Vec<HashMap<usize, (i32,i32)>>,

    pub grounding: bool,

    pub new_limits: [f64; 3],

    pub logging: bool,

    pub senario: i32, 
    
    pub enroll_algo_version: i32,

    pub mean_yield_rate: f64,
}

impl Config {
    ///////////////////////////////////////////////////////
    // 定数定義
    //大学入試マトリクスの各生成値の意味
    pub const APPLY: u8 = 1;   //受験（学生）
    pub const ENROLL_1ST: u8 = 2;  //私立合格一次（大学）
    pub const RESERVE: u8 = 4;  //入学金納付し入学保留（学生）
    pub const ADMISSION_1ST: u8 = 8;  //私立入学先先行決定（学生）
    pub const ENROLL_2ND: u8 = 16; //国公立合格（大学）
    pub const ADMISSION_2ND: u8 = 32; //保留先へ入学決定（学生）
    pub const ENROLL_3RD: u8 = 64; //追加合格（大学）
    pub const ADMISSION_3RD: u8 = 128; //追加合格大学へ入学（学生）

    //大学入試結果resultマトリクス集計時の意味
    pub const R_FAILED: u8 = 1;  //不合格
    pub const R_DECLINE1: u8 = 3;  //辞退
    pub const R_DECLINE1_PAID: u8 = 7;  //入学金納付後辞退
    pub const R_DECLINE2: u8 = 65;  //追加合格辞退
    pub const R_RESERVED: u8 = 7;   //入学金納付後保留中
    pub const R_ENROLL_3RD: u8 = 65;  //追加合格
    pub const R_ADMISSION_1ST: u8 = 11; //一次合格で私立入学
    pub const R_ADMISSION_2ND: u8 = 17; //国公立に合格し入学
    pub const R_ADMISSION_RSV: u8 = 39; //一次合格保留後私立入学
    pub const R_ADMISSION_3RD: u8 = 193; //追加入学決定

    //大学設定区分
    // pub const NATIONAL: u8 = 1; //国立
    // pub const PUBLIC: u8 = 2; //公立
    pub const PRIVATE: u8 = 3; //私立

    //入学定員超過率の年度別上限  <= 2021まで
    pub const MAX_ENROLLMENT_RATES: [[f64; 3]; 7] = [
        // 大学規模L M S
        [1.20, 1.30, 1.30], // < 2016
        [1.17, 1.27, 1.30], //  2016
        [1.14, 1.24, 1.30], //  2017
        [1.10, 1.20, 1.30], //  2018
        [1.10, 1.20, 1.30], //  2019
        [1.10, 1.20, 1.30], //  2020
        [1.10, 1.20, 1.30], //  2021
    ];

    ///////////////////////////////////////////////////////
    // ここから関数定義
    // Configオブジェクト生成。　コマンドライン引数の設定ファイルから。
    // Configオブジェクトは一度だけstaticで生成され、その後不変。
    pub fn from_args() -> Result<()>{
        let matches = App::new("大学受験戦略シミュレーション")
            .version(crate_version!())
            .arg(Arg::with_name("CONFIG_FILE") 
                .help("設定ファイル名")
                .required(true)
            )
            .arg(Arg::with_name("seed")              // フラグを定義
                .help("random seed")                // ヘルプメッセージ
                .short("s")                         // ショートコマンド
                .long("seed")                       // ロングコマンド
                .takes_value(true)
            )
            .arg(Arg::with_name("logging")              // フラグを定義
                .help("output log csv")                // ヘルプメッセージ
                .short("l")                         // ショートコマンド
                .long("log")                       // ロングコマンド
            )
            .get_matches();

        if let Some(filename) = matches.value_of("CONFIG_FILE") {
            let mut f = fs::File::open(filename).expect("config toml file not found");
            eprintln!("    設定ファイル = {:?}", filename);

            let mut contents = String::new();
            f.read_to_string(&mut contents).expect("config file read error");
            let mut cfg: Config = toml::from_str(&contents).unwrap();

            // 2021.12.08 ランダムシードの指定があれば設定ファイルの指定を上書き
            if let Some(seed) = matches.value_of("seed"){
                cfg.random_seed = seed.parse::<u64>().unwrap();
            }

            // 2021.12.23 ログ出力指定があれば設定ファイルの指定を上書き
            if matches.is_present("logging"){
                cfg.logging = true;
            }
            if cfg.logging{
                cfg.output_dir = Config::get_output_dirname(& cfg)?;
                eprintln!("    ログ出力先Dir = {:?}", cfg.output_dir);
            } else {
                eprintln!("    ログ出力なし");
            }

            eprintln!("    random seed = {:?}", cfg.random_seed);

            // 2021.11.23 接地用　2年目以降定員情報Vec作成
            if cfg.grounding {
                cfg.enroll_capa_dics = Config::make_enroll_capa_info(&cfg)?;
            }

            //設定ファイルを出力先Dirにコピー
            // if cfg.logging{
            //     fs::copy(filename, format!("{}/{}",cfg.output_dir, filename)).unwrap();
            // }

            CONFIG.set(cfg).unwrap();
            Ok(())
        } else {
            eprintln!("設定ファイル名が指定されていません。");
            exit(1)
        }
    }

    // 生成済みのConfigオブジェクトを返す
    pub fn get() -> &'static Config{
        CONFIG.get().expect("Not initalized Config")
    }

    //データ出力ディレクトリを生成し、その相対パス名を返す。
    pub fn get_output_dirname(conf: &Config) -> Result<String>{
        let prefix = format!("s{0}r{1:<04}_", conf.senario, conf.random_seed);
        let new_dir = conf.output_dir_base.clone() + "/" + &prefix + &Local::now().format("%Y_%m%d_%H%M%S").to_string();
        match fs::create_dir(new_dir.clone()).context("dir cannot create"){
            Err(e) => Err(e),
            Ok(_) => Ok(new_dir + "/"),
        }
    }

    // 2021.11.23 定員情報作成
    pub fn make_enroll_capa_info(&self) -> Result<Vec<HashMap<usize, (i32,i32)>>>{
        let mut v = vec![];
        for i in 1..self.epochs{
            let mut h = HashMap::new();
            let year = self.start_year + i as usize;
            let path = format!("{}{:04}{}", self.enroll_capa_csv_dir, year, self.enroll_capa_csv_name);
            let mut rdr = ReaderBuilder::new().from_path(path)?;
            for result in rdr.deserialize(){
                let e: EnrollAndCapa = result?;
                h.insert(e.cid, (e.enroll, e.capa));
            }
            v.push(h);
        }
        Ok(v)
    }
}