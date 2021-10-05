use crate::college::{College, Cid};
use crate::config::Config;
use crate::Matrix;

use rayon::prelude::*;
use rand::Rng;
use rand::distributions::{Distribution,WeightedIndex};
use rand_distr::{Normal, StandardNormal};
use rand::seq::index::sample;
use rand_xoshiro::rand_core::SeedableRng;
use rand_xoshiro::Xoshiro256StarStar;
use superslice::Ext;
use std::collections::{HashSet, HashMap};

pub type Sid = usize; //学生ID

#[derive(Debug,Clone)]
pub enum ApplyPattern {
    NationalPublicOnly, //国公立のみ
    Both, // 国公立私立併願
    PrivateOnly, //私立専願
}


#[derive(Debug,Clone)]
pub struct Student{
    pub id: Sid,
    pub score: i32, //偏差値を1000倍した整数
    pub c_map: HashMap<usize, i32>, //出願した大学のインデックスと、試験成績のマップ
    pub pattern: ApplyPattern, //併願パターン　
    pub admission: Option<Cid>, //入学を決めた大学のインデックス
    pub rng: Xoshiro256StarStar,
}

impl Student {
    pub fn new(fscore: f64, prev_rng: &mut Xoshiro256StarStar) -> Self{
        prev_rng.jump();
        Self{id: 0,
            score: (fscore * 1000.0).round() as i32,
            c_map: HashMap::new(),
            pattern: ApplyPattern::Both,
            admission: None,
            rng: prev_rng.clone()
        }
    }

    pub fn from_conf(conf: &Config) -> Vec<Self> {
        let mut rng1 = Xoshiro256StarStar::seed_from_u64(conf.random_seed);
        let normal = Normal::new(50.0, 10.0).unwrap();
        let choice = [ApplyPattern::NationalPublicOnly, ApplyPattern::Both, ApplyPattern::PrivateOnly];
        let pattern = WeightedIndex::new(&conf.apply_pattern_rate).unwrap();

        let mut students: Vec<Self> = normal.sample_iter(&mut rng1)
            .take(conf.student_number)
            .collect::<Vec<f64>>()
            .into_iter()
            .map(|x| Student::new(x, &mut rng1))
            .collect();
        students.par_sort_unstable_by(|a, b| a.score.cmp(&b.score));
        students.into_par_iter()
            .enumerate()
            .map(|(i, mut x)| {
                //連番をIDとして設定。偏差値が低いほど若い。
                x.id = i;
                //併願パターンを決定
                x.pattern = choice[pattern.sample(&mut x.rng)].clone();
                x
            })
            .collect()
    }
    
    // ランク別の大学グループを作成し、各グループから受験大学を選択して出願＆受験する。
    // ランクの範囲、数、各ランクから何校選ぶかはconfigの設定に従う。
    // 試験結果として誤差を加えた自分の偏差値を大学インデックスをキーとしたMapに保存する。
    pub fn apply(&mut self, conf: &Config, nationals: &[College], privates: &[College]) -> (Vec<(usize, usize)>, Vec<Cid>){
        
        let mut c_vec: Vec<Cid> = Vec::new(); //選択した大学
        let mut bounds: Vec<(usize, usize)> = Vec::new();//私立大学ランク範囲
        let mut national: Option<Cid> = None;

        match self.pattern {
            ApplyPattern::Both | ApplyPattern::NationalPublicOnly => {
                // 1:国公立を１校選択
                national = self.from_nationals(conf, nationals);
            },
            _ => (),
        }
        match self.pattern {
            ApplyPattern::Both | ApplyPattern::PrivateOnly => {
                // 2:私立大学から複数選択
                let rank_num = conf.college_rank_lower.len();
                bounds = (0..rank_num).into_iter()
                    .map(|i| self.get_bounds(conf.college_rank_lower[i], conf.college_rank_upper[i], privates))
                    .collect();
                // println!("inner:bounds:{:?}",bounds);
                c_vec = (0..rank_num).into_iter()
                    .map(|i| self.select_college(conf, privates, bounds[i], conf.college_rank_select_number[i]))
                    .flatten()
                    .collect::<HashSet<Cid>>() //一旦Setにして重複を削除
                    .into_iter()
                    .collect();
            },
            _ => (),
        }
        // 3:国公立があれば配列に追加
        if let Some(n) = national{
            c_vec.push(n);
        }
        // 4:試験　大学毎の試験成績を記録
        c_vec.iter().for_each(|c_idx| {
            let exam_result = self.exam();
            self.c_map.insert(*c_idx, exam_result);
        });
        (bounds, c_vec)
    }

    // 国公立大学から1校選択
    pub fn from_nationals(&mut self, conf: &Config, nationals: &[College]) -> Option<Cid>{
        let bounds: (usize, usize) = self.get_bounds(conf.national_range[0], conf.national_range[1], nationals);
        // println!("inner:bounds:{:?}",bound);
        match bounds{
            // 偏差値に合う国公立なし
            (0, 0) => None,
            _ => {// 大学グループから一様分布で1校だけ大学を選択
                let size = (bounds.1 as i32) - (bounds.0 as i32) + 1;
                if size <= 1 {
                    Some(nationals[bounds.0].index)
                } else {
                    let idx = sample(&mut self.rng, size as usize, 1).index(0);
                    Some(nationals[idx + bounds.0].index)
                }
            }
        }
    }


    // 大学ランク別グループの下限と上限（配列のインデックス）を返す。
    fn get_bounds(&self, lower: i32, upper: i32, colleges: &[College]) -> (usize, usize){
        let max_size = colleges.len();
        let mut lower = colleges.lower_bound_by_key(&(self.score + lower * 1000),|x| x.score);
        let mut upper = colleges.upper_bound_by_key(&(self.score + upper * 1000),|x| x.score);
        if upper != 0{
            upper = upper - 1;
        } 
        if lower >= max_size{
            lower = lower - 1;
        }
        (lower, upper)
    }

    // 私立大学ランク別グループから、configでグループ別に指定された数だけ出願校を選択する。
    fn select_college(&mut self, conf: &Config, colleges: &[College], bound: (usize, usize), select_number: usize) -> Vec<usize>{
        let mut v: Vec<usize> = Vec::new();
        let size = (bound.1 as i32) - (bound.0 as i32) + 1;
        //上限と下限が同値、1校しかなかった場合、
        if size <= 1 {
            v.push(bound.0);
        // 大学数が出願数以下だった場合。
        }else if size <= select_number as i32{
            (bound.0..=bound.1).into_iter().for_each(|x| v.push(x));
        // 大学グループから一様分布で出願数だけ大学を選択
        }else {
            v = sample(&mut self.rng, size as usize, select_number).into_vec()
                .iter()
                .map(|x| x + bound.0).collect();
        }
        //私立大学配列上のインデクスから、その先の大学全体のインデックスに変換してから値を返す
        v.iter().map(|x| colleges[*x].index).collect()
    }

    // 入学試験。自分の偏差値 + 標準正規分布誤差を返す。
    fn exam(&mut self) -> i32{
        self.score + (self.rng.sample::<f32, _>(StandardNormal) * 1000.0).round() as i32
    }

    // 合格した大学から入学する大学を選択。
    pub fn admission2(&mut self, _conf: &Config, colleges: &[College], matrix: &Matrix, idx: Sid) -> Option<Cid>{
        let statuss: Vec<(Cid,Option<&u8>)> = matrix.outer_view(idx).unwrap().indices().iter()
            .map(|col| (*col, matrix.get(idx, *col)))
            .collect();

        let mut passed_colleges: Vec<College> = Vec::new();
        for (cid, val) in statuss {
            if let Some(v) = val {
                match *v{
                    //第一志望（私立または国公立）に合格している
                    Config::R_ADMISSION_1ST | Config::R_ADMISSION_2ND => {
                        self.admission = Some(cid);
                        return None //既に決定しているので
                    },
                    //一次合格保留中か追加合格の大学
                    Config::R_RESERVED | Config::R_ENROLL_3RD => {
                        passed_colleges.push(colleges[cid].clone());
                    },
                    _ => (),
                }
            }
        }

        if passed_colleges.len() == 0 {
            return None
        }

        passed_colleges.sort_unstable_by(|a, b| b.score.cmp(&a.score));
        self.admission = Some(passed_colleges[0].index);
        self.admission
    }

    //入学決定１　志望校合格時に入学 or 入学金納付のみ or パス
    pub fn admission1(&mut self, _conf: &Config, colleges: &[College], passed_ids: &[Cid]) -> Vec<(usize, (usize, u8))>{
        let select_college: Cid;
        match passed_ids.len(){
            0 => Vec::<(usize, (usize, u8))>::new(), //合格大学なし
            _ => { 
                let mut apply_colleges: Vec::<&College> = self.c_map.iter()
                    .map(|(key, _)| &colleges[*key])
                    .collect();
                apply_colleges.sort_unstable_by(|a, b| b.score.cmp(&a.score));
                select_college = apply_colleges[0].index;
    

                //意中の大学には入学、それ以外には保留の値をもつベクトルを返す
                passed_ids.iter()
                    .map(|cid|{
                        (*cid, (self.id, if *cid == select_college {
                                 Config::ADMISSION_1ST
                                }else{
                                 Config::RESERVE
                                }))})
                    .collect()
            }
        }
    }

    //指定大学の受験時点数（偏差値）を取得
    pub fn exam_dev(&self, cid: Cid) -> &i32{
        self.c_map.get(&cid).unwrap()
    }
}