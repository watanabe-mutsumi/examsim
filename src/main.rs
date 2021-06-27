mod univ;
mod student;
mod config;

use rand::distributions::Distribution;
use statrs::distribution::Normal;
use rayon::prelude::*;
use std::time::Instant;
use anyhow::Result;
use std::sync::mpsc::channel;

use crate::univ::{Univ, Uid};
use crate::student::{Student, Sid};
use crate::config::Config;

pub fn main() -> Result<()>{

    let begin = Instant::now();

    Config::create_from_args()?;

    let mut rng = rand::thread_rng();
    let n = Normal::new(50.0, 10.0).unwrap();

    let mut univs: Vec<Univ> = Univ::from_path(&Config::get().initial_univ_csv)?;

    let students: Vec<Student> = (0..Config::get().student_number).into_iter()
        .map(|_x|n.sample(&mut rng))
        .collect::<Vec<f64>>()
        .into_par_iter()
        .map(|x| Student::new(x))
        .collect();

        
    let (sender, receiver) = channel();

    let mut students: Vec<Student>  = students.into_par_iter()
        .map_with((sender, &univs),|(s,u),mut x| {
            x.flg = Config::get().student_number as i32;
            s.send(x.entry(u)).unwrap();
            x
        })
        .collect();

    let b: Vec<_> = receiver.iter() 
        .collect();

    // 偏差値の高い順にソート
    students.par_sort_unstable_by(|a, b| b.score.cmp(&a.score));

    let entires: Vec<(Uid, Sid)> = vec![
        (0,1), (1,3), (0,2), (3,5)
    ];

    entires.into_iter()
        .for_each(|(u,s)| univs[u].s_vec.push(s));

    println!("2015 L rate: {:?}", config::MAX_ENROLLMENT_RATES[3][univs[0].univ_scale()]);
    println!("Unv[0] {:?}", univs[0]);
    println!("config sutudent num {:?}", Config::get().student_number);
    println!("collected Studnets: {:?}", b.len());
    println!("elaspled:{:?}", begin.elapsed());

    Ok(())
}