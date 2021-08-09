use crate::college::{College, Cid};
use crate::config::Config;

use rayon::prelude::*;
use rand::Rng;
use rand::distributions::Distribution;
use rand_distr::{Normal, StandardNormal};
use rand::seq::index::sample;
use rand_xoshiro::rand_core::SeedableRng;
use rand_xoshiro::Xoshiro256StarStar;
use superslice::Ext;
use std::collections::{HashSet, HashMap};

pub type Sid = usize; //学生ID

#[derive(Debug,Clone)]
pub struct Student{
    pub id: Sid,
    pub score: i32, //偏差値を1000倍した整数
    pub c_map: HashMap<usize, i32>, //出願した大学のインデックスと、試験成績のマップ
    pub admission: Option<Cid>, //入学を決めた大学のインデックス
    pub rng: Xoshiro256StarStar,
}

impl Student {
    pub fn new(fscore: f64, prev_rng: &mut Xoshiro256StarStar) -> Self{
        prev_rng.jump();
        Self{id: 0,
            score: (fscore * 1000.0).round() as i32,
            c_map: HashMap::new(),
            admission: None,
            rng: prev_rng.clone()
        }
    }

    pub fn from_conf(conf: &Config) -> Vec<Self> {
        let mut rng1 = Xoshiro256StarStar::seed_from_u64(conf.random_seed);
        let normal = Normal::new(50.0, 10.0).unwrap();

        let mut students: Vec<Self> = normal.sample_iter(&mut rng1)
            .take(conf.student_number)
            .collect::<Vec<f64>>()
            .into_iter()
            .map(|x| Student::new(x, &mut rng1))
            .collect();
        students.par_sort_unstable_by(|a, b| b.score.cmp(&a.score));
        students.into_par_iter()
            .enumerate()
            .map(|(i, mut x)| {x.id = i; x})
            .collect()
    }
    
    // ランク別の大学グループを作成し、各グループから受験大学を選択して出願＆受験する。
    // ランクの範囲、数、各ランクから何校選ぶかはconfigの設定に従う。
    // 試験結果として誤差を加えた自分の偏差値を大学インデックスをキーとしたMapに保存する。
    pub fn apply(&mut self, conf: &Config, colleges: &[College]) -> (Vec<(usize, usize)>, Vec<Cid>){
        let rank_num = conf.college_rank_lower.len();
        let bounds: Vec<(usize, usize)> = (0..rank_num).into_iter()
            .map(|i| self.get_bounds(conf.college_rank_lower[i], conf.college_rank_upper[i], colleges))
            .collect();
        // println!("inner:bounds:{:?}",bounds);
        let c_vec: Vec<Cid> = (0..rank_num).into_iter()
            .map(|i| self.select_college(bounds[i], conf.college_rank_select_number[i]))
            .flatten()
            .collect::<HashSet<Cid>>() //一旦Setにして重複を削除
            .into_iter()
            .collect();
        //大学毎の試験成績を記録
        c_vec.iter().for_each(|c_idx| {
            let exam_result = self.exam();
            self.c_map.insert(*c_idx, exam_result);
        });
        (bounds, c_vec)
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

    // 大学ランク別グループから、configでグループ別に指定された数だけ出願校を選択する。
    fn select_college(&mut self, bound: (usize, usize), select_number: usize) -> Vec<usize>{
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
        v
    }

    // 入学試験。自分の偏差値 + 標準正規分布誤差を返す。
    fn exam(&mut self) -> i32{
        self.score + (self.rng.sample::<f32, _>(StandardNormal) * 1000.0).round() as i32
    }

    // 合格した大学から入学する大学を選択。
    pub fn admission(&mut self, _conf: &Config, colleges: &[College], passed_ids: &[Cid]) -> Option<Cid>{
        match passed_ids.len(){
            0 => None, //合格大学なし
            1 => { // 暫定。1校しか合格しなかったのでそこに入学
                self.admission = Some(passed_ids[0]);
                Some(passed_ids[0])
            },
            _ => { // 国公立(institute=1or2)があればそこに入学、なければ最も偏差値の高い大学を選択
                let mut passed_colleges: Vec<College> = passed_ids.into_iter()
                    .map(|x| colleges[*x].clone())
                    .collect();
                passed_colleges.sort_unstable_by(|a, b| b.score.cmp(&a.score));
                let national_public: Vec<College> = passed_colleges.clone().into_iter()
                    .filter(|x| x.institute < Config::PRIVATE)
                    .collect();
                let seletcted = if let 0 = national_public.len() {
                    passed_colleges[0].index
                } else {
                    national_public[0].index
                };
                self.admission = Some(seletcted);
                Some(seletcted)
            },
        }
    }

}
 
// #[test]
// fn test_apply() -> Result<()>{
//     Config::from_path("config01.toml")?;

//     let colleges: Vec<College> = College::from_conf(&Config::get())?;
//     let students: Vec<Student> = Student::from_conf(&Config::get());

//     println!("Collges 0:{:?}", colleges[0]);
//     println!("Student 0:{:?}", students[0]);

//     Ok(())
// }


