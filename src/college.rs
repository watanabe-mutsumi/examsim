use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use anyhow::Result;
use csv::ReaderBuilder;
use std::cmp::min;

use crate::student::{Student, Sid};
use crate::config::Config;
use crate::Matrix;

pub type Cid = usize; //大学ID

#[derive(Debug,Clone,Default,Deserialize,Serialize)]
pub struct College{
    #[serde(default)]
    pub senario: i32, //2021.12.11 本番シミュレーションシナリオ番号．1-5. 0は設置
    #[serde(default)]
    pub epoch: usize, //イテレーション番号。0オリジン。
    #[serde(default)]
    pub seed: u64, //2021.12.11 ランダムシード

    pub cid: Cid, //旺文社の大学番号
    pub name: String,  //  大学名
    pub institute: u8, // 設置区分：1国立 2公立 3私立
    pub pref: u8, // 都道府県番号：1-47
    pub urban: String, //都市区分表記："地方"or"都市圏"
    pub capa: u32, // 大学全体の収容数
    pub dev: f64, // 偏差値
    pub enroll: u32, //　入学定員数
    pub over_rate: f64, //合格者超過率
    pub applicant_num: u32, // 2021.11.29 前年度志願者数
    pub passed_num: u32, // 2021.12.12 前年度合格者数
    pub adm_num: u32, // 2021.12.12 前年度入学者数
    #[serde(default)]
    pub own_scale: usize, // 2021.12.11 大学規模

    #[serde(default)]
    pub current_rate: f64, // 2021.11.29 現在の入学定員超過率制限値

    #[serde(default)]
    pub score: i32, //ソート用に1000倍して整数化した偏差値
    #[serde(default)]
    pub index: usize, //ソート後の連番。これが配列のインデックスになる。
    #[serde(default)]
    pub saved: bool, //定員割れで公立として救済されたかのフラグ
    #[serde(default)]
    #[serde(skip_serializing)]
    pub s_vec: Vec<Sid>, //一次合格した受験生のインデックス
    #[serde(default)]
    pub new_enroll_num: usize, //今回の一次合格者総数最大値。私立用。
    #[serde(default)]
    pub add_enroll_num: usize, //今回の追加合格用人数。私立用。

    #[serde(default)]
    pub dev_history: Vec<f64>, //各ステップの偏差値履歴。
    #[serde(default)]
    pub adm_history: Vec<i32>, //各ステップの入学者数履歴。
    #[serde(default)]
    pub fillrate_history: Vec<f64>, //各ステップの定員従属率履歴。



    // #[serde(default = "crate::college::default_rng")]
    // pub rng: dyn SeedableRng,
}

impl College {
    pub fn from_conf(conf: &Config) -> Result<Vec<Self>>{
        let mut colleges: Vec<College> = Vec::new();
        let mut rdr = ReaderBuilder::new().from_path(&conf.initial_college_csv)?;
        for result in rdr.deserialize(){
            let mut college: Self = result?;
            college.score = (college.dev * 1000.0).round() as i32;
            college.seed = conf.random_seed;
            college.senario = conf.senario;
            college.dev_history.push(college.dev);//シミュレーション前の偏差値
            college.fillrate_history.push(0.0);//シミュレーション前の充足率は0にしておく
            college.adm_history.push(0);//シミュレーション前の入学者数は0にしておく
            // 2021.11.29 志願者数が0（欠損値）の場合は入学定員を代用する
            if college.applicant_num == 0 {
                college.applicant_num = college.enroll;
            }

            colleges.push(college);
        }
        // 偏差値の昇順にソート
        colleges.par_sort_by(|a, b| a.score.cmp(&b.score));
        for i in 0..colleges.len() {colleges[i].index = i}
        Ok(colleges)    
    }

    //1ステップ分の入試結果を反映した新しいエージェントを返す
    pub fn update(&self, result: &CollegeResult) -> College{
        let mut college = self.clone();
        college.epoch += 1; //1年分進める
        //フラグtrue時のみ大学偏差値を入学者偏差値で更新する
        if Config::get().update_dev {
            college.dev = result.new_deviation;
        }
        college.score = (college.dev * 1000.0).round() as i32;
        //2021.12.12 入学者数，合格者数を更新
        college.adm_num = result.admissons as u32;
        college.passed_num = result.enroll_1st_count as u32 + result.enroll_add_count as u32;

        college.dev_history.push(result.new_deviation);
        //定員充足率　入学者÷定員
        college.fillrate_history.push( result.admissons as f64 / college.enroll as f64 );
        //2021.12.11 入学者履歴
        college.adm_history.push(result.admissons as i32);

        // 2021.11.29 志願者数更新
        college.applicant_num = result.apply_count as u32;

        //次年度の合格者超過率
        //入試結果を元に次年度のあるべき（辞退者が出ても入学定員になる）定員超過率を計算
        //合格者数/入学者数
        //2021.11.22 学習率を導入し、変化量*lrだけ増減させる。
        college.over_rate = result.enroll_1st_count  as f64 /  result.admissons as f64; 

        // 2021.11.23 接地の場合、2年目以降の新しい入学定員、収容定員を設定する。最終年度は不要。
        if Config::get().grounding && college.epoch < Config::get().epochs as usize{
            if let Some((new_enroll, new_capa)) = Config::get().enroll_capa_dics[self.epoch].get(&self.cid){
                college.enroll = *new_enroll as u32;
                college.capa = *new_capa as u32;
            }
        }

        college
    }

    //私立一次合格者決定 
    pub fn enroll1(&mut self, conf: &Config, students: &[Student], candidates: &[usize]) -> Vec<Sid>{
        // 1.前年度実績と今年度入学定員制限から合格者数を決定。
        self.new_enroll_num = self.enroll_num();
        // 追加合格用人数を設定
        self.add_enroll_num = (self.new_enroll_num as f64 * conf.enroll_add_rate).round() as usize;
        self.new_enroll_num -=  self.add_enroll_num; //追加合格分を引く
        // eprintln!("add_enroll_num:{:?}", self.add_enroll_num);

        // ２。受験者の配列を取得。
        let mut id_and_scores: Vec<(&usize, &i32)> = candidates.into_iter()
            .map(|x| (x, students[*x].c_map.get(&self.index).unwrap()))
            .collect();
        //3.成績の良い順に合格者を決定
        id_and_scores.sort_by(|a, b| b.1.cmp(a.1));
        // println!("Coll idx: {:?} enroll_num:{:?} id&score.len :{:?}", self.index, enroll_num, id_and_scores.len());
        let s_vec = (0..min(id_and_scores.len(), self.new_enroll_num)).into_iter()
            .map(|x|*(id_and_scores[x].0)).collect::<Vec<Sid>>();
        //4. 合格者を記録
        self.s_vec = s_vec.clone();
        s_vec
    }

    //国公立合格者決定 
    pub fn enroll2(&mut self, students: &[Student], candidates: &[usize]) -> Vec<Sid>{
        // 1.前年度実績と今年度入学定員制限から合格者数を決定。
        self.new_enroll_num = self.enroll_num();
        // ２。受験者の配列を取得。
        let mut id_and_scores: Vec<(&usize, &i32)> = candidates.into_iter()
            .map(|x| (x, students[*x].c_map.get(&self.index).unwrap()))
            .collect();
        //3.成績の良い順に合格者を決定
        id_and_scores.sort_by(|a, b| b.1.cmp(a.1));
        // println!("Coll idx: {:?} enroll_num:{:?} id&score.len :{:?}", self.index, enroll_num, id_and_scores.len());
        let s_vec = (0..min(id_and_scores.len(), self.new_enroll_num)).into_iter()
            .map(|x|*(id_and_scores[x].0)).collect::<Vec<Sid>>();
        //4. 合格者を記録
        self.s_vec = s_vec.clone();
        s_vec
    }

    //私立追加合格者決定 
    pub fn enroll3(&mut self, conf: &Config, students: &[Student], matrix: &Matrix, idx: usize) -> Vec<Sid>{
        // 1.現在の入学者数を計算
        let statuss: Vec<(usize,u8)> = matrix.outer_view(idx).unwrap().indices().into_iter()
            .map(|col| (*col, *matrix.get(idx, *col).unwrap()))
            .collect();
        let current_admisson_num = statuss.iter()
            .filter(|(_, val)|  *val == Config::R_ADMISSION_1ST ||
                                *val == Config::R_ADMISSION_RSV )
            .count();
        
        // 2021.11.29 入学定員でなく、入学定員×定員超過率の数値をベースにする。
        let base_line = (self.enroll as f64 * self.current_rate).ceil() as usize;
        
        let diff = if current_admisson_num > base_line{
                0 as usize
            }else{
                base_line  - current_admisson_num
            };
        if  diff > 0 { //不足
            //差分に追加合格用人数を上乗せ
            let limit = diff + self.add_enroll_num;
            //偏差値足切り値
            let limit_score = if conf.enroll_add_lower == 0{
                    0 //偏差値足切りなし
                }else{
                    self.score  - conf.enroll_add_lower
                };
            // ２。受験者のうち、未だ合格させていない者から追加合格者候補リストを作成
            let mut id_and_scores: Vec<(&usize, &i32)> = statuss.iter()
                .filter(|(_, val)| *val == Config::R_FAILED)
                .map(|(x, _)| (x, students[*x].c_map.get(&self.index).unwrap()))
                .collect();
            //3.成績の良い順に合格者を決定
            id_and_scores.sort_by(|a, b| b.1.cmp(a.1));
            // println!("Coll idx: {:?} enroll_num:{:?} id&score.len :{:?}", self.index, enroll_num, id_and_scores.len());
            let s_vec = (0..min(id_and_scores.len(), limit)).into_iter()
                // 2021.10.5 偏差値による足切り
                .filter(|x| *(id_and_scores[*x].1) >= limit_score)
                .map(|x|*(id_and_scores[x].0)).collect::<Vec<Sid>>();
            s_vec
        } else {
            Vec::new()
        }
    }

    // 今年度の合格者数を計算
    fn enroll_num(&mut self) -> usize{
        //2021.11.21 私立のみ変化。国公立は1.0固定
        if self.institute != Config::PRIVATE {
            return self.enroll as usize;
        }

        self.own_scale = self.college_scale();
        let before_current: (usize, usize);
        let this_year = Config::get().start_year + self.epoch;
        let mut limit_table = Config::MAX_ENROLLMENT_RATES.to_vec();
        limit_table.push(Config::get().new_limits);
        if !Config::get().small_college_support { // 2021.11.19 小規模優遇なし
            before_current = match this_year{
                0..=2015    => (0,0),//変化なし
                2016..=2018 => (this_year - 2016, this_year - 2015),
                _ => (3,3), //変化なし
            };
        } else {
            before_current = match this_year{
                0..=2015    => (0,0),//変化なし
                2016..=2022 => (this_year - 2016, this_year - 2015),
                _ => (7,7), //変化なし
            };
        };
        self.current_rate = limit_table[before_current.1][self.own_scale];

        //2021.12.12 アルゴリズム改善．旧バージョンも残す
        if Config::get().enroll_algo_version == 2 {
            // 歩留率計算．入学者数または合格者数が欠損（0）の場合は2014年度の私立大学平均を使う
            let yield_rate =  if self.passed_num == 0 || self.adm_num == 0{
                    Config::get().mean_yield_rate
                } else {
                    self.adm_num as f64 / self.passed_num as f64
                };
            ((self.enroll as f64 * self.current_rate) / yield_rate).round() as usize
            
        } else {
            // 2016(0)以前と2016(1)の増減率を取得。
            let limit_change_rate = self.current_rate / limit_table[before_current.0][self.own_scale];        
            ((self.enroll as f64) * self.over_rate * limit_change_rate).round() as usize
        }
    }

    fn college_scale(&self) -> usize {
        if self.capa >= 8000 { 0 }
        else if self.capa >= 4000{ 1 }
        else { 2 }
    }
    
}

// シミュレーション結果CSV
#[derive(Debug, Default, Clone, Serialize)]
pub struct CollegeResult{
    //エポック数
    pub epoch: i32,
    //基本情報　初期値から不変
    pub index: usize, //偏差値昇順ソート後の連番。配列のインデックス
    pub cid: Cid, //旺文社の大学番号
    pub name: String,  //  大学名
    pub institute: u8, // 設置区分：1国立 2公立 3私立
    pub dev: f64, // 偏差値
    pub enroll: u32, //　入学定員数
    pub over_rate: f64, //合格者超過率

    //シミュレーション結果
    pub apply_count: i32, //受験者数
    pub decline1: i32, //辞退
    pub decline2: i32, //追加合格辞退
    
    pub enroll_1st_count: i32, //正規合格数
    pub enroll_add_count: i32, //追加合格数
    pub paid_only_count: i32, //入学金納付のみ

    pub admisson_1st: i32, //一次、国立合格で入学
    pub admisson_rsv: i32, //一次保留後入学
    pub admisson_add: i32, //追加合格入学

    pub admissons: i32, //最終入学者数
    pub new_deviation: f64, //入学者偏差値平均
    pub payments: i32, //入学金徴収総額
}

// 2021.11.23 入学定員・収容人数CSV
#[derive(Debug,Clone,Default,Deserialize)]
pub struct EnrollAndCapa{
    pub cid: Cid, // 旺文社大学番号

    #[serde(alias = "入学定員数")]
    pub enroll: i32, //入学定員

    #[serde(alias = "収容定員数")]
    pub capa: i32,  //大学全体の収容人数
}
