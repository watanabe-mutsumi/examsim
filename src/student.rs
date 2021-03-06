use crate::college::{College, Cid};
use crate::config::Config;
use crate::Matrix;

use rayon::prelude::*;
use rand::Rng;
use rand::distributions::{Distribution,WeightedIndex};
use rand_distr::Normal; //StandardNormal
use rand::seq::index::sample;
use rand_xoshiro::rand_core::SeedableRng;
use rand_xoshiro::Xoshiro256StarStar;
use superslice::Ext;
use std::collections::{HashSet, HashMap};
use serde::Serialize;

pub type Sid = usize; //学生ID

#[derive(Debug,Clone)]
pub enum ApplyPattern {
    Both = 1, // 国公立私立併願
    PrivateOnly = 2, //私立専願
}


#[derive(Debug,Clone)]
pub struct Student{
    pub id: Sid,
    pub score: i32, //偏差値を1000倍した整数
    pub c_map: HashMap<usize, i32>, //出願した大学のインデックスと、試験成績のマップ
    pub pattern: ApplyPattern, //併願パターン　
    pub admission: Option<Cid>, //入学を決めた大学のインデックス
    pub rng: Xoshiro256StarStar, //乱数生成器
}

impl Student {
    pub fn new(fscore: f64, seed: u64) -> Self{
        Self{id: 0,
            score: (fscore * 1000.0).round() as i32,
            c_map: HashMap::new(),
            pattern: ApplyPattern::Both,
            admission: None,
            rng: Xoshiro256StarStar::seed_from_u64(seed)
        }
    }

    pub fn from_conf(conf: &Config, epoch: usize) -> Vec<Self> {
        let mut rng1 = Xoshiro256StarStar::seed_from_u64(conf.random_seed);
        let normal = Normal::new(conf.student_dev_mu, conf.student_dev_sigma).unwrap();

        let mut students: Vec<Self> = normal.sample_iter(&mut rng1)
            .take(conf.student_number[epoch]) //今回の年度の志願者数分生成
            .collect::<Vec<f64>>()
            .into_iter()
            .enumerate()
            .map(|(i, x)| Student::new(x, conf.random_seed + i as u64))
            .collect();
        students.par_sort_by(|a, b| a.score.cmp(&b.score));
        students.into_par_iter()
            .enumerate()
            .map(|(i, mut x)| {
                //連番をIDとして設定。偏差値が低いほど若い。
                x.id = i;
                //併願パターンを決定
                x.pattern = if x.rng.gen_bool(conf.national_prob){
                    ApplyPattern::Both //国立も受験
                } else {
                    ApplyPattern::PrivateOnly
                };
                x
            })
            .collect()
    }
    
    // ランク別の大学グループを作成し、各グループから受験大学を選択して出願＆受験する。
    // ランクの範囲、数、各ランクから何校選ぶかはconfigの設定に従う。
    // 試験結果として誤差を加えた自分の偏差値を大学インデックスをキーとしたMapに保存する。
    pub fn apply(&mut self, conf: &Config, nationals: &[College], privates: &[College]) -> (Vec<(usize, usize)>, Vec<Cid>){
        
        let mut c_vec:Vec<Cid> ; //選択した大学
        let bounds: Vec<(usize, usize)> ;//私立大学ランク範囲
        let mut national: Option<Cid> = None;

        //出願数のパターンをランダムに選択
        let pattern: usize = if self.rng.gen_bool(conf.first_pattern_rate){
            0 //延べ6校
        } else{
            1 //延べ7校
        };

        match self.pattern {
            ApplyPattern::Both => {
                // 1:国公立を１校選択
                national = self.from_nationals(conf, nationals);
            },
            _ => (),
        }
        // 2:私立大学から複数選択
        // 2021.12.31 偏差値平均以下の学生はダイヤモアンド型受験
        let select_nums = if self.score as f64 / 1000.0  > conf.student_dev_mu{
            conf.college_rank_select_number.to_vec()
        }else{
            conf.college_rank_select_number_diamond.to_vec()
        };

        let rank_num = conf.college_rank_lower.len();
        bounds = (0..rank_num).into_iter()
            .map(|i| self.get_bounds(conf.college_rank_lower[i], conf.college_rank_upper[i], privates))
            .collect();
        // println!("inner:bounds:{:?}",bounds);
        // ABC大学ランク毎に指定選択数だけ大学を選ぶ
        c_vec = (0..rank_num).into_iter()
            .map(|i| {
                //Aランク(i==0)時、国公立にも出願する場合には選択数をその分１つ減らす
                let mut select_number = select_nums[pattern][i];
                match national{
                    None =>  (), //そのまま
                    _ => if i == 0 { select_number -= 1 } //１校分減らす
                }
                self.select_college(privates, bounds[i], select_number)
            })
            .flatten()
            .collect::<HashSet<Cid>>() //一旦Setにして重複を削除
            .into_iter()
            .collect();
        
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
            _ => {// 大学グループから入学定員に比例した確率または一様分布で1校だけ大学を選択
                let size = (bounds.1 as i32) - (bounds.0 as i32) + 1;
                if size <= 1 {
                    Some(nationals[bounds.0].index)
                } else {
                    let idx_v = self.random_select(Config::get().college_select_by_enroll,
                         size as usize, 1, bounds.0, nationals);
                    Some(nationals[idx_v[0]].index)
                }
            }
        }
    }


    // 大学ランク別グループの下限と上限（配列のインデックス）を返す。
    fn get_bounds(&self, lower: i32, upper: i32, colleges: &[College]) -> (usize, usize){
        //2021.12.31 学生偏差値上限と下限の緩和
        let dev = if Config::get().college_dev_rift &&
            self.score - 5000 < Config::get().college_dev_lower{
            Config::get().college_dev_lower + 5000
        }else if self.score + 5000 > Config::get().college_dev_upper{
            Config::get().college_dev_upper - 5000
        }else {
            self.score
        };

        let max_size = colleges.len();
        let mut lower = colleges.lower_bound_by_key(&(dev + lower * 1000),|x| x.score);
        let mut upper = colleges.upper_bound_by_key(&(dev + upper * 1000),|x| x.score);
        if upper != 0{
            upper = upper - 1;
        } 
        if lower >= max_size{
            lower = lower - 1;
        }
        (lower, upper)
    }

    // 私立大学ランク別グループから、configでグループ別に指定された数だけ出願校を選択する。
    fn select_college(&mut self, colleges: &[College], bound: (usize, usize), select_number: usize) -> Vec<usize>{
        let mut v: Vec<usize> = Vec::new();
        let size = (bound.1 as i32) - (bound.0 as i32) + 1;
        //上限と下限が同値、1校しかなかった場合、
        if size <= 1 {
            v.push(bound.0);
        // 大学数が出願数以下だった場合。
        }else if size <= select_number as i32{
            (bound.0..=bound.1).into_iter().for_each(|x| v.push(x));
        // 大学グループから入学定員に比例した確率または一様分布で出願数だけ大学を選択
        }else {
            v = self.random_select(Config::get().college_select_by_enroll, size as usize, select_number, bound.0, colleges);
        }
        //私立大学配列上のインデクスから、その先の大学全体のインデックスに変換してから値を返す
        v.iter().map(|x| colleges[*x].index).collect()
    }

    // 入学試験。自分の偏差値 + 標準正規分布誤差を返す。
    // 2021.12.01 誤差をN(0, 生成時標準偏差^2)に変更
    fn exam(&mut self) -> i32{
        // self.score + (self.rng.sample::<f32, _>(StandardNormal) * 1000.0).round() as i32
        let normal = Normal::new(0.0, Config::get().student_dev_sigma).unwrap(); // 2021.12.11 正規分布生成器
        self.score + (normal.sample(& mut self.rng) * 100.0).round() as i32 // 2021.12.24揺らぎを10%にする
    }

    //一様分布又は入学定員に比例した確率で大学を選択
    // 2021.11.29 入学定員ではなく志願者数に比例させる
    fn random_select(&mut self, proportional: bool, size: usize, select_number: usize, offset: usize, colleges: &[College]) -> Vec<usize>{
        if proportional{
            let mut v: Vec<usize> = Vec::new();
            let choice = (0..size).into_iter().map(|x|x + offset).collect::<Vec<usize>>();
            let weight = choice.iter().map(|x| colleges[*x].applicant_num).collect::<Vec<u32>>();
            let dist = WeightedIndex::new(weight).unwrap();
            while v.len() < select_number{
                let bingo = choice[dist.sample(&mut self.rng)];
                if v.is_empty() || !v.contains(&bingo){
                    v.push(bingo);
                }
            }
            v
        } else {
            sample(&mut self.rng, size as usize, select_number).into_vec()
                .iter()
                .map(|x| x + offset).collect()
        }
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
                apply_colleges.sort_by(|a, b| b.score.cmp(&a.score));
                select_college = apply_colleges[0].index;
    

                //合格大学に対し、私立専願で、最上位の大学が合格なら入学、それ以外には保留の値をもつベクトルを返す
                let reuslt_vec = passed_ids.iter()
                    .map(|cid|{
                        (*cid, (self.id, 
                                if *cid == select_college {
                                // 2021.12.01 私立専願なら決定、国公立併願なら保留
                                    match self.pattern {
                                        ApplyPattern::PrivateOnly => {
                                            self.admission = Some(*cid);
                                            Config::ADMISSION_1ST
                                            },
                                        _ => Config::RESERVE,
                                    }
                                }else{
                                    Config::RESERVE
                                }
                                )
                        )})
                    .collect();
                //2021.12.30 私立専願で志望校合格でも保留するバグの修正
                if let Some(id) = self.admission{
                    vec![(id, (self.id,  Config::ADMISSION_1ST)) ]
                }else{
                    reuslt_vec
                }
            }
        }
    }

    // 国公立合格発表を受けて入学大学を選択．国公立に合格なら入学．
    // 国公立不合格の場合，入学保留中の大学のあれば最高偏差値の私立へ入学すことにしてその大学indexを返す．
    // 戻り値：国公立合格の場合，即入学決定なのでNone,国公立不合格で保留中大学もない場合もNone
    pub fn admission2(&mut self, conf: &Config, colleges: &[College], matrix: &Matrix, idx: Sid) -> Option<Cid>{
        let statuss: Vec<(Cid,Option<&u8>)> = matrix.outer_view(idx).unwrap().indices().iter()
            .map(|col| (*col, matrix.get(idx, *col)))
            .collect();

        let mut reserved_colleges: Vec<&College> = Vec::new();
        for (cid, val) in statuss {
            if let Some(v) = val {
                match *v{
                    //国公立に合格している
                    Config::R_ADMISSION_2ND => {
                        self.admission = Some(cid);
                        return None //既に決定したので
                    },
                    //一次合格保留中の大学
                    Config::R_RESERVED => {
                        reserved_colleges.push(&colleges[cid]);
                    },
                    _ => (),
                }
            }
        }

        if reserved_colleges.len() == 0 {
            return None
        }

        // 2022.1.2 保留大学を捨てて追加合格に賭ける場合，パスする
        if self.rng.gen_bool(conf.wait_addtion_rate){
            return None
        }

        //保留中の大学から最も偏差値の高い大学に入学
        reserved_colleges.sort_by(|a, b| b.score.cmp(&a.score));
        self.admission = Some(reserved_colleges[0].index);
        self.admission
    }

    // 追加合格した大学から入学する大学を選択。
    pub fn admission3(&mut self, _conf: &Config, colleges: &[College], matrix: &Matrix, idx: Sid) -> Option<Cid>{
        let statuss: Vec<(Cid,Option<&u8>)> = matrix.outer_view(idx).unwrap().indices().iter()
            .map(|col| (*col, matrix.get(idx, *col)))
            .collect();

        let mut passed_colleges: Vec<&College> = Vec::new();
        for (cid, val) in statuss {
            if let Some(v) = val {
                match *v{
                    //私立第一志望または国公立また保留していた大学に合格している
                    // Config::R_ADMISSION_1ST | Config::R_ADMISSION_2ND |
                    // Config::R_ADMISSION_RSV => { // 2021.12.29 追加
                    //     self.admission = Some(cid);
                    //     return None //既に決定しているので
                    // },
                    //追加合格の大学
                    Config::R_ENROLL_3RD => {
                        passed_colleges.push(&colleges[cid]);
                    },
                    _ => (),
                }
            }
        }

        if passed_colleges.len() == 0 {
            return None
        }

        passed_colleges.sort_by(|a, b| b.score.cmp(&a.score));
        self.admission = Some(passed_colleges[0].index);
        self.admission
    }

    //指定大学の受験時点数（偏差値）を取得
    // pub fn exam_dev(&self, cid: Cid) -> &i32{
    //     self.c_map.get(&cid).unwrap()
    // }
}

// シミュレーション結果CSV
#[derive(Debug, Clone, Serialize)]
pub struct StudentResult{ 
    pub epoch: i32, //エポック数
    pub id: Sid,
    pub score: i32, //偏差値を1000倍した整数
    pub pattern: u8, //出願パターン
    pub college: i32, //2021.12.29 入学した大学の連番．全滅の場合は-1
    pub result: String, // cid:value_cid:value
}

// 受験結果マトリクスを１学生１行の形式にしたデバック用受験生入試結果ベクターを作成
pub fn settle(epoch: i32, students: &[Student], smap: &mut HashMap<Sid,Vec<(Cid, u8)>>, colleges: &[College])  -> Vec<StudentResult>{
   students.par_iter()
        .map(|s| StudentResult{
            epoch: epoch,
            id: s.id,
            score: s.score,
            pattern: s.pattern.clone() as u8,
            college: match s.admission{Some(cid)=>cid as i32, _ => -1},
            result: if let Some(c_vec) = smap.get(&s.id){
                        c_vec.iter()
                            .map(|(cid, status)| 
                                format!("{}:{}:{}", colleges[*cid].institute, cid, *status) )
                            .collect::<Vec<_>>()
                            .join(" ")
                    } else {//受験せず
                        "".to_string()
                    }
            }
        )
        .collect()
}