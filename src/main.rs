mod college;
mod student;
mod config;

use rayon::prelude::*;
use rayon_croissant::ParallelIteratorExt;
use sprs::{TriMat, CsMatBase};
use sprs::io::write_matrix_market;
use std::time::Instant;
use chrono::Local;
use anyhow::{Context, Result};

use crate::college::{College, Cid};
use crate::student::{Student, Sid};
use crate::config::Config;

pub type Matrix = CsMatBase<u8, usize, Vec<usize>, Vec<usize>, Vec<u8>, usize>;

pub fn main() -> Result<()>{

    let begin = Instant::now();
    println!("大学受験シミュレーション　Ver. 1.0");
    println!("開始 {}",Local::now());

    // 設定ファイルからグローバルなConfigオブジェクトを作成
    // Config::from_args()?;
    Config::from_path("config01.toml")?;

    //Step:0 大学エージェントと学生（受験生）エージェントを作成
    let colleges: Vec<College> = College::from_conf(&Config::get())?;
    let students: Vec<Student> = Student::from_conf(&Config::get());
    
    //Step:1 出願 & 試験（学生行動）
    let (apply_matrix, mut students) = apply(students, &colleges);
    
    //Step:2 合否判定（大学行動）
    let (enroll_matrix, mut colleges) = enroll(colleges, &students, &apply_matrix);

    //Step:3 入学先決定（学生行動）
    let (admisson_matrix, mut students) = admission(students, &colleges, &enroll_matrix);

    //Step:4 シミュレーション結果をファイルに保存
    save(&apply_matrix, enroll_matrix, &admisson_matrix)?;


    println!("colleges len:{:?}", colleges.len());
    println!("studnets len:{:?}", students.len());

    let sid = 56_000;
    println!("students[56_000] {:?}", students[sid]);
    let (bounds, selection) = students[sid].apply(&Config::get(), &colleges);
    println!("bounds:{:?}",bounds);
    println!("selection:{:?}",selection);
    let sid = 0;
    println!("students[0] {:?}", students[sid]);
    let (bounds, selection) = students[sid].apply(&Config::get(), &colleges);
    println!("bounds:{:?}",bounds);
    println!("selection:{:?}",selection);
    let sid = 559_999;
    println!("students[559_999] {:?}", students[sid]);
    let (bounds, selection) = students[sid].apply(&Config::get(), &colleges);
    println!("bounds:{:?}",bounds);
    println!("selection:{:?}",selection);

    println!("終了 {}",Local::now());
    println!("elaspled:{:?}", begin.elapsed());
    Ok(())
}

// 大学選択　＆　受験
fn apply(students: Vec<Student>, colleges: &[College]) -> (Matrix, Vec<Student>){
    let mut apply_list: Vec<(usize, usize)> = Vec::new();
    let s2 = students.into_par_iter()
        .mapfold_reduce_into(
        &mut apply_list,
        |acc, mut x|{
            let idx = x.id;
            let (_, entries) = x.apply(&Config::get(), colleges);
            for college_idx in entries { acc.push((college_idx, idx));}
            x
        },
        Default::default,
        |left, mut right| left.append(&mut right)
    ).collect::<Vec<Student>>();

    // 出願sparseマトリクス　行=大学、列=受験生、値1(出願) を作成
    let apply_matrix = make_matrix(&apply_list, colleges.len(), s2.len(), 1);
    (apply_matrix, s2)
}

// 合格者決定
fn enroll(colleges: Vec<College>, students: &[Student], apply_mat: &Matrix) -> (Matrix, Vec<College>){
    let mut enroll_list: Vec<(usize, usize)> = Vec::new();
    let c2 = colleges.into_par_iter()
        .mapfold_reduce_into(
        &mut enroll_list,
        |acc, mut x|{
            let idx = x.index;
            let entries = x.enroll(students, apply_mat.outer_view(idx).unwrap().indices());
            for student_idx in entries {acc.push((student_idx, idx));}
            x
        },
        Default::default,
        |left, mut right| left.append(&mut right)
    ).collect::<Vec<College>>();

    // 合格sparseマトリクス　行=受験生、列=大学、値2(合格) を作成
    let enroll_matrix = make_matrix(&enroll_list, students.len(), c2.len(), 2);
    (enroll_matrix, c2)
}

// 入学大学決定
fn admission
    (students: Vec<Student>, colleges: &[College], enroll_mat: &Matrix) -> (Matrix, Vec<Student>) {
    let mut admission_list: Vec<(Cid, Sid)> = Vec::new();
    let s2 = students.into_par_iter()
        .mapfold_reduce_into(
        &mut admission_list,
        |acc, mut x|{
            let idx = x.id;
            if let Some(college_idx) = x.admission(&Config::get(), colleges, 
                        enroll_mat.outer_view(idx).unwrap().indices()){
                acc.push((college_idx, idx));
            }
            x
        },
        Default::default,
        |left, mut right| left.append(&mut right)
    ).collect::<Vec<Student>>();

    // 入学者sparseマトリクス　行=大学、列=受験生、値４(入学) を作成
    let admission_matrix = make_matrix(&admission_list, colleges.len(), s2.len(), 4);
    (admission_matrix, s2)
}

// sparseマトリクス作成　行=受験生or大学、列=大学or受験生、値 を作成
fn make_matrix(list: &[(usize, usize)], rows: usize, cols: usize, value: u8) -> Matrix{
    let mut trimat = TriMat::new((rows, cols));
    list.iter().for_each(|(row, col)| trimat.add_triplet(*row, *col, value));
    trimat.to_csr()
}

//シミュレーション結果をファイルに保存
fn save(apply_matrix: &Matrix, enroll_matrix: Matrix, admisson_matrix: &Matrix) -> Result<()>{
    let filename = &"test_sparse.mm";
    let tran_enroll_matrix = enroll_matrix.transpose_into();
    let temp_matrix = apply_matrix + &tran_enroll_matrix;
    let result_matrix = &temp_matrix + admisson_matrix;
    write_matrix_market(filename, &result_matrix)
        .context( format!("Matrix file {} can not save.", filename))
}
