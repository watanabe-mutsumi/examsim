use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use anyhow::Result;
use csv::ReaderBuilder;
use std::cmp::min;

use crate::student::{Student, Sid};
use crate::config::Config;

pub type Cid = usize; //大学ID

#[derive(Debug,Clone,Deserialize)]
pub struct College{
    pub cid: Cid, //旺文社の大学番号
    pub name: String,  //  大学名
    pub institute: u8, // 設置区分：1国立 2公立 3私立
    pub pref: u8, // 都道府県番号：1-47
    pub urban: String, //都市区分表記："地方"or"都市圏"
    pub capa: u32, // 大学全体の収容数
    pub dev: f64, // 偏差値
    pub enroll: u32, //　入学定員数
    pub over_rate: f64, //合格者超過率
    #[serde(default)]
    pub score: i32, //ソート用に1000倍して整数化した偏差値
    #[serde(default)]
    pub index: usize, //ソート後の連番。これが配列のインデックスになる。
    #[serde(default)]
    pub s_vec: Vec<Sid>, //合格した受験生のインデックス
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
            colleges.push(college);
        }
        // 偏差値の昇順にソート
        colleges.par_sort_unstable_by(|a, b| a.score.cmp(&b.score));
        for i in 0..colleges.len() {colleges[i].index = i}
        Ok(colleges)    
    }

    //合格者決定 
    pub fn enroll(&mut self, students: &[Student], candidates: &[usize]) -> Vec<Sid>{
        // 1.前年度実績と今年度入学定員制限から合格者数を決定。
        let enroll_num = self.enroll_num();
        // ２。受験者の配列を取得。
        let mut id_and_scores: Vec<(&usize, &i32)> = candidates.into_iter()
            .map(|x| (x, students[*x].c_map.get(&self.index).unwrap()))
            .collect();
        //3.成績の良い順に合格者を決定
        id_and_scores.sort_unstable_by(|a, b| b.1.cmp(a.1));
        // println!("Coll idx: {:?} enroll_num:{:?} id&score.len :{:?}", self.index, enroll_num, id_and_scores.len());
        let s_vec = (0..min(id_and_scores.len(), enroll_num)).into_iter()
            .map(|x|*(id_and_scores[x].0)).collect::<Vec<Sid>>();
        //4. 合格者を記録
        self.s_vec = s_vec.clone();
        s_vec
    }

    // 今年度の合格者数を計算
    fn enroll_num(&self) -> usize{
        let own_scale = self.college_scale();
        // 暫定: 2016(0)以前と2016(1)の増減率を取得。本当は毎年変わる。
        let limit_change_rate = 
            Config::MAX_ENROLLMENT_RATES[1][own_scale]/Config::MAX_ENROLLMENT_RATES[0][own_scale];
        ((self.enroll as f64) * self.over_rate * limit_change_rate).ceil() as usize
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
    //基本情報　初期値から不変
    pub index: usize, //偏差値昇順ソート後の連番。配列のインデックス
    pub cid: Cid, //旺文社の大学番号
    pub name: String,  //  大学名
    pub institute: u8, // 設置区分：1国立 2公立 3私立
    pub dev: f64, // 偏差値
    pub enroll: u32, //　入学定員数
    pub over_rate: f64, //合格者超過率

    //シミュレーション結果
    pub apply_count: usize, //受験者数
    pub enroll_1st_count: usize, //正規合格数
    pub enroll_sup_count: usize, //追加合格数
    pub paid_only_count: usize, //入学金納付のみ
    pub payments: usize, //入学金徴収総額
    pub addmissons: usize, //最終入学者数
    pub new_deviation: f64, //入学者偏差値平均
    pub new_over_rate: f64, //新合格者超過率
    pub admisson_over_rate: f64, //入学定員超過率 
}