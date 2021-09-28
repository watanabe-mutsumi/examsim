mod college;
mod student;
mod config;

use rayon::prelude::*;
use sprs::{TriMat, CsMatBase};
use std::time::Instant;
use std::io::{stdout, Error};
use chrono::Local;
use anyhow::{Result};

use crate::college::{College, Cid, CollegeResult};
use crate::student::{Student, Sid};
use crate::config::Config;

pub type Matrix = CsMatBase<u8, usize, Vec<usize>, Vec<usize>, Vec<u8>, usize>;

pub fn main() -> Result<()>{

    let begin = Instant::now();
    eprintln!("大学入試シミュレーション　Ver. 1.0");
    eprintln!("開始 {}",Local::now());

    // 設定ファイルからグローバルなConfigオブジェクトを作成
    // Config::from_args()?;
    Config::from_path("config01.toml")?;

    //Step:0 大学エージェントと学生（受験生）エージェントを作成
    let mut colleges: Vec<College> = College::from_conf(&Config::get())?;
    let mut students: Vec<Student> = Student::from_conf(&Config::get());
    eprintln!("    0: 初期化完了&シミュレーション開始 \t{:?}",begin.elapsed());

    //国公立と私立大学に分けたベクターを用意
    let (nationals, privates) = divide_colleges(&colleges);

    //Step:1 出願 & 試験（学生行動）
    let apply_matrix = apply(&mut students, &nationals, &privates);
    
    //Step:2 合否判定（大学行動）
    let enroll_matrix = enroll(&mut colleges, &students, &apply_matrix);

    //Step:3 入学先決定（学生行動）
    let admisson_matrix  = admission(&mut students, &colleges, &enroll_matrix);
    eprintln!("    1: シミュレーション完了&データ保存開始 \t{:?}",begin.elapsed());

    //Step:4 シミュレーション結果を標準出力にCSV形式で出力
    print_result(&students, &colleges, &apply_matrix, enroll_matrix, &admisson_matrix)?;

    let debug : bool = false;
    if debug == true {
        eprintln!("colleges len:{:?}", colleges.len());
        eprintln!("studnets len:{:?}", students.len());
        eprintln!("nationals len:{:?}", nationals.len());
        eprintln!("privates len:{:?}", privates.len());

        let sid = 56_000;
        eprintln!("students[56_000] {:?}", students[sid]);
        let (bounds, selection) = students[sid].apply(&Config::get(), &nationals, &privates);
        eprintln!("bounds:{:?}",bounds);
        eprintln!("selection:{:?}",selection);
        let sid = 0;
        eprintln!("students[0] {:?}", students[sid]);
        let (bounds, selection) = students[sid].apply(&Config::get(),&nationals, &privates);
        eprintln!("bounds:{:?}",bounds);
        eprintln!("selection:{:?}",selection);
        let sid = 559_999;
        eprintln!("students[559_999] {:?}", students[sid]);
        let (bounds, selection) = students[sid].apply(&Config::get(), &nationals, &privates);
        eprintln!("bounds:{:?}",bounds);
        eprintln!("selection:{:?}",selection);

        eprintln!("{:?}",colleges[77]);
        eprintln!("{:?}",colleges[737]);
    }

    eprintln!("終了 {}",Local::now());
    eprintln!("elaspled:{:?}", begin.elapsed());
    Ok(())
}

// 大学選択　＆　受験
fn apply(students: &mut Vec<Student>, nationals: &[College], privates: &[College]) -> Matrix{
    let apply_list: Vec<(usize, usize)> = students.par_iter_mut()
        .fold_with( Vec::new(),
            |mut acc, x|{
                let idx = x.id;
                let (_, entries) = x.apply(&Config::get(), nationals, privates);
                for college_idx in entries { acc.push((college_idx, idx));};
                acc
        })
        .reduce( || Vec::new(),
          |mut left, mut right| {
              left.append(&mut right);
              left
        });

    // 出願sparseマトリクス　行=大学、列=受験生、値1(出願) を作成
    make_matrix(&apply_list, nationals.len() + privates.len(), students.len(), Config::APPLY)
}

// 合格者決定
fn enroll(colleges:&mut Vec<College>, students: &[Student], apply_mat: &Matrix) -> Matrix{
    let enroll_list: Vec<(usize, usize)> = colleges.par_iter_mut()
        .fold_with(Vec::new(),
            |mut acc, x|{
                let idx = x.index;
                let entries = x.enroll(students, apply_mat.outer_view(idx).unwrap().indices());
                for student_idx in entries {acc.push((student_idx, idx));}
                acc
        })
        .reduce( || Vec::new(),
        |mut left, mut right| {
            left.append(&mut right);
            left
        });

    // 合格sparseマトリクス　行=受験生、列=大学、値2(合格) を作成
    make_matrix(&enroll_list, students.len(), colleges.len(), Config::ENROLL)
}

// 入学大学決定
fn admission
    (students: &mut Vec<Student>, colleges: &[College], enroll_mat: &Matrix) -> Matrix { 
    let admission_list: Vec<(Cid, Sid)> = students.par_iter_mut()
        .fold_with( Vec::new(),
            |mut acc, x|{
                let idx = x.id;
                if let Some(college_idx) = x.admission(&Config::get(), colleges, 
                            enroll_mat.outer_view(idx).unwrap().indices()){
                    acc.push((college_idx, idx));
                }
                acc
        })
        .reduce( || Vec::new(),
          |mut left, mut right| {
              left.append(&mut right);
              left
        });

    // 入学者sparseマトリクス　行=大学、列=受験生、値４(入学) を作成
    make_matrix(&admission_list, colleges.len(), students.len(), Config::ADMISSION)
}

// sparseマトリクス作成　行=受験生or大学、列=大学or受験生、値 を作成
fn make_matrix(list: &[(usize, usize)], rows: usize, cols: usize, value: u8) -> Matrix{
    let mut trimat = TriMat::new((rows, cols));
    list.iter().for_each(|(row, col)| trimat.add_triplet(*row, *col, value));
    trimat.to_csr()
}

//シミュレーション結果をstdoutにCSV形式で出力
fn print_result(students: &Vec<Student>, colleges: &Vec<College>,
    apply_matrix: &Matrix, enroll_matrix: Matrix, admisson_matrix: &Matrix) -> Result<()>{
    
    let tran_enroll_matrix = enroll_matrix.transpose_into();
    let temp_matrix = apply_matrix + &tran_enroll_matrix;
    let result_matrix = &temp_matrix + admisson_matrix;

    let mut wtr = csv::Writer::from_writer(stdout());

    for x in colleges{
        let mut apply_count: usize = 0; //受験者数
        let mut enroll_1st_count: usize = 0; //一次合格者数
        let mut admission_count: usize = 0;  //入学者数
        let mut new_dev: f64 = 0.0; //入学者の偏差値合計
       

        // eprintln!("college.index:{} enroll_count:{}", x.index, tran_enroll_matrix.outer_view(x.index).unwrap().indices().len());
            

        result_matrix.outer_view(x.index).unwrap().indices().iter()
            .for_each(|col|
                if let Some(val) = result_matrix.get(x.index, *col){
                    match *val{
                        Config::R_PASSED => enroll_1st_count += 1,
                        Config::R_ADMISSION => {
                            enroll_1st_count += 1;
                            admission_count += 1;
                            new_dev += students[*col].score as f64 / 1000.0;
                        },
                        _ => (),
                    }
                    apply_count += 1;
                }
            );

        wtr.serialize(CollegeResult {
            index: x.index, //偏差値昇順ソート後の連番。配列のインデックス
            cid: x.cid, //旺文社の大学番号
            name: x.name.clone(),  //  大学名
            institute: x.institute, // 設置区分：1国立 2公立 3私立
            dev: x.dev, // 偏差値
            enroll: x.enroll, //　入学定員数
            over_rate: x.over_rate, //合格者超過率
        
            //シミュレーション結果
            apply_count: apply_count, //受験者数
            enroll_1st_count: enroll_1st_count, //合格者総数
            addmissons: admission_count, //最終入学者数
            //入学者偏差値平均
            new_deviation: new_dev / admission_count as f64,
            ..Default::default()

            // enroll_sup_count: usize, //追加合格数
            // paid_only_count: usize, //入学金納付のみ
            // payments: usize, //入学金徴収総額
            // new_over_rate: f64, //新合格者超過率
            // admisson_over_rate: f64, //入学定員超過率 
        })?;
    };

    wtr.flush()?;
    Ok(())
}

//国公立と私立を分ける
fn divide_colleges(colleges: &[College]) -> (Vec<College>, Vec<College>){
    let privates: Vec<College> = colleges.iter().cloned()
        .filter(|x| x.institute == Config::PRIVATE)
        .collect();
    let nationals: Vec<College> = colleges.iter().cloned()
        .filter(|x| x.institute != Config::PRIVATE)
        .collect();
    (nationals, privates)
}
