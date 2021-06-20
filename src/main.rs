extern crate rand;
extern crate statrs;
extern crate rayon;


// use rand::Rng;
use rand::distributions::Distribution;
use statrs::distribution::Normal;
use rayon::prelude::*;
use std::time::Instant;

type Sid = usize; //学生ID
type Uid = usize; //大学ID

pub fn get_univ_scale(capa: usize) -> usize {
    if capa >= 8000 { 0 }
    else if capa >= 4000{ 1 }
    else { 2 }
}

const MAX_ENROLLMENT_RATES: [[f64; 3]; 4] = [
    // 大学規模L M S
    [1.20, 1.30, 1.30], // < 2016
    [1.17, 1.27, 1.30], //  2016
    [1.14, 1.24, 1.30], //  2017
    [1.10, 1.20, 1.30], //  2018 ~
];

const STUDENTS_NUM: usize = 560_000; //受験生数　初期値
// const MAX_ENROLLMENT_RATES: [f32; 5] = []

#[derive(Debug,Clone)]
pub(crate) struct Student{
    id: Sid,
    score: i32, //偏差値を1000倍した整数
    u_vec: Vec<Univ>,
}

impl Student {
    pub fn new(i: f64) -> Self{
        Self{id: 0,
            score: (i * 1000.0).round() as i32,
            u_vec: Vec::new()
        }
    }
}


#[derive(Debug,Clone)]
pub(crate) struct Univ{
    id: Uid,
    s_vec: Vec<Sid>
}

impl Univ {
   pub fn new(i: Uid) -> Self{
       Self {id: i, s_vec: Vec::new()}
   } 
}

// fn entry(us: &mut Vec<Univ>, e: (Uid, Sid)){
//     let (uid, sid) = e;
//     us[uid].s_vec.push(sid);
// }

pub fn main() {
    let begin = Instant::now();

    let mut rng = rand::thread_rng();
    let n = Normal::new(50.0, 10.0).unwrap();

    let mut students: Vec<Student> = (0..STUDENTS_NUM).into_iter()
        .map(|_x|n.sample(&mut rng))
        .collect::<Vec<f64>>()
        .into_par_iter()
        .map(|x| Student::new(x))
        .collect();
    // 偏差値の高い順にソート
    students.par_sort_unstable_by(|a, b| b.score.cmp(&a.score));

    let mut univs: Vec<Univ> = (0..5).into_iter()
        .map(|u| Univ::new(u))
        .collect();

    let entires: Vec<(Uid, Sid)> = vec![
        (0,1), (1,3), (0,2), (3,5)
    ];

    entires.into_iter()
        .for_each(|(u,s)| univs[u].s_vec.push(s));

    println!("2015 L rate: {:?}", MAX_ENROLLMENT_RATES[3][get_univ_scale(8000)]);
    println!("日本語3");
    println!("学生の先頭 {:?}", students[0]);
    println!("学生の先頭 {:?}", students[1]);
    println!("学生の先頭 {:?}", students[STUDENTS_NUM-1]);
    println!("elaspled:{:?}", begin.elapsed());
}
