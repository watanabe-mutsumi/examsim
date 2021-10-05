mod college;
mod student;
mod config;

use rayon::prelude::*;
use sprs::{TriMat, CsMatBase};
use std::time::Instant;
use std::collections::HashMap;
use std::io::stdout;
use chrono::Local;
use anyhow::{Result};

use crate::college::{College, Cid, CollegeResult};
use crate::student::{Student, Sid, ApplyPattern};
use crate::config::Config;

pub type Matrix = CsMatBase<u8, usize, Vec<usize>, Vec<usize>, Vec<u8>, usize>;
pub type SidStatus = (usize, u8);

pub fn main() -> Result<()>{

    let begin = Instant::now();
    eprintln!("大学入試シミュレーション　Ver. 1.0");
    eprintln!("開始 {}",Local::now());

    // 設定ファイルからグローバルなConfigオブジェクトを作成
    Config::from_args()?;
    // Config::from_path("config01.toml")?;

    //Step:0 大学エージェントと学生（受験生）エージェントを作成
    let mut colleges: Vec<College> = College::from_conf(&Config::get())?;
    let mut students: Vec<Student> = Student::from_conf(&Config::get());
    eprintln!("    0: 初期化完了&シミュレーション開始 \t{:?}",begin.elapsed());

    //国公立と私立大学に分けたベクターを用意
    let (nationals, privates) = divide_colleges(&colleges);
 
    //Step:1 出願 & 試験（学生行動）
    let apply_matrix = apply(&mut students, &nationals, &privates);
   
    //Step:2 私立一次合格発表（大学行動）
    let enroll1_matrix = enroll1(&mut colleges, &students, &apply_matrix);

    //Step:3 入学判定１回目（学生行動）
    let adm1_matrix  = admission1(&mut students, &colleges, &enroll1_matrix);

    //Step:4 国公立合格発表（大学行動）
    let enroll2_matrix = enroll2(&mut colleges, &students, &apply_matrix);

    //状態遷移マトリクス集計
    let temp1 = &apply_matrix + &(enroll1_matrix.transpose_into());
    let status1 = &temp1 + &adm1_matrix;

    //Step:5 私立追加合格発表（大学行動）
    let enroll3_matrix = enroll3(&mut colleges, &students, &status1);

    //状態遷移マトリクス集計
    let temp2 = &enroll2_matrix  + &(status1.transpose_into());
    let status2 = &temp2 + &enroll3_matrix;

    //Step:6 入学先最終決定（学生行動）
    let adm2_matrix  = admission2(&mut students, &colleges, &status2);
    eprintln!("    1: シミュレーション完了&データ保存開始 \t{:?}",begin.elapsed());

    //Step:4 シミュレーション結果を標準出力にCSV形式で出力
    print_result(&students, &colleges, status2, &adm2_matrix)?;
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

// 合格者決定1　私立のみ
fn enroll1(colleges:&mut Vec<College>, students: &[Student], apply_mat: &Matrix) -> Matrix{
    let enroll_list: Vec<(usize, usize)> = colleges.par_iter_mut()
        .filter(|x| x.institute == Config::PRIVATE)
        .fold_with(Vec::new(),
            |mut acc, x|{
                let idx = x.index;
                let entries = x.enroll1(students, apply_mat.outer_view(idx).unwrap().indices());
                for student_idx in entries {acc.push((student_idx, idx));}
                acc
        })
        .reduce( || Vec::new(),
        |mut left, mut right| {
            left.append(&mut right);
            left
        });

    // 合格sparseマトリクス　行=受験生、列=大学、値2(合格) を作成
    make_matrix(&enroll_list, students.len(), colleges.len(), Config::ENROLL_1ST)
}

//  入学決定1回名。私立大学のみ。志望校合格時に入学 or 入学金納付のみ or パス
 fn admission1
    (students: &mut Vec<Student>, colleges: &[College], enroll_mat: &Matrix) -> Matrix { 
    let new_list: Vec<(Cid, SidStatus)> = students.par_iter_mut()
        .filter(|x| match x.pattern{
            ApplyPattern::Both | ApplyPattern::PrivateOnly => true,
            _ => false,
        })
        .fold_with( Vec::new(),
            |mut acc, x|{
                let idx = x.id;
                let mut entries = x.admission1(&Config::get(), colleges, enroll_mat.outer_view(idx).unwrap().indices());
                acc.append(&mut entries);
                acc
        })
        .reduce( || Vec::new(),
          |mut left, mut right| {
              left.append(&mut right);
              left
        });

    // 入学金納付者sparseマトリクス　行=大学、列=受験生、値４(入学金納付のみ) or 8（入学）を作成
    make_matrix_any_value(&new_list, colleges.len(), students.len())
}

// 合格者決定２　国公立合格発表。合格者は入学も決定する。
fn enroll2(colleges:&mut Vec<College>, students: &[Student], mat: &Matrix) -> Matrix{
    let new_list: Vec<(usize, usize)> = colleges.par_iter_mut()
        .filter(|x| x.institute != Config::PRIVATE) //国公立のみ   
        .fold_with(Vec::new(),
            |mut acc, x|{
                let idx = x.index;
                let entries = x.enroll2(students, mat.outer_view(idx).unwrap().indices());
                for student_idx in entries {acc.push((student_idx, idx));}
                acc
        })
        .reduce( || Vec::new(),
        |mut left, mut right| {
            left.append(&mut right);
            left
        });

    // 合格sparseマトリクス　行=受験生、列=大学、値2(合格) を作成
    make_matrix(&new_list, students.len(), colleges.len(), Config::ENROLL_2ND)
}

// 合格者決定３　私立追加合格発表。
fn enroll3(colleges:&mut Vec<College>, students: &[Student], mat: &Matrix) -> Matrix{
    let new_list: Vec<(usize, usize)> = colleges.par_iter_mut()
        .filter(|x| x.institute == Config::PRIVATE) //私立のみ   
        .fold_with(Vec::new(),
            |mut acc, x|{
                let idx = x.index;
                let entries = x.enroll3(students, mat, idx);
                for student_idx in entries {acc.push((student_idx, idx));}
                acc
        })
        .reduce( || Vec::new(),
        |mut left, mut right| {
            left.append(&mut right);
            left
        });

    // 合格sparseマトリクス　行=受験生、列=大学、値32(追加合格) を作成
    make_matrix(&new_list, students.len(), colleges.len(), Config::ENROLL_3RD)
}


// 入学大学最終決定
fn admission2
    (students: &mut Vec<Student>, colleges: &[College], mat: &Matrix) -> Matrix { 
    let admission_list: Vec<(Cid, Sid)> = students.par_iter_mut()
        .fold_with( Vec::new(),
            |mut acc, x|{
                let idx = x.id;
                if let Some(college_idx) = x.admission2(&Config::get(), colleges, mat, idx){
                    acc.push((college_idx, idx));
                }
                acc
        })
        .reduce( || Vec::new(),
          |mut left, mut right| {
              left.append(&mut right);
              left
        });

    // 入学者sparseマトリクス　行=大学、列=受験生、値64(最終決定入学先) を作成
    make_matrix(&admission_list, colleges.len(), students.len(), Config::ADMISSION_2ND)
}

// sparseマトリクス作成　行=受験生or大学、列=大学or受験生、値 を作成
fn make_matrix(list: &[(usize, usize)], rows: usize, cols: usize, value: u8) -> Matrix{
    let mut trimat = TriMat::new((rows, cols));
    list.iter().for_each(|(row, col)| trimat.add_triplet(*row, *col, value));
    trimat.to_csr()
}

// sparseマトリクス作成　行=受験生or大学、列=大学or受験生、値はlistから取得
fn make_matrix_any_value(list: &[(usize, SidStatus)], rows: usize, cols: usize) -> Matrix{
    let mut trimat = TriMat::new((rows, cols));
    list.iter().for_each(|(row, col)| trimat.add_triplet(*row, col.0, col.1));
    trimat.to_csr()
}


//シミュレーション結果をstdoutにCSV形式で出力
fn print_result(students: &Vec<Student>, colleges: &Vec<College>, status: Matrix, adm2: &Matrix) -> Result<()>{
    
    let tran_matrix = status.transpose_into();
    let result_matrix = adm2 + &tran_matrix ;

    let mut wtr = csv::Writer::from_writer(stdout());

    for x in colleges{
        let mut new_dev: f64 = 0.0; //入学者の偏差値合計
        let mut counters = HashMap::new();
        let values = result_matrix.outer_view(x.index).unwrap().indices().iter()
            .map(|col|{
                if let Some(val) = result_matrix.get(x.index, *col){
                    //状態値別に件数を集計
                    let counter = counters.entry(*val).or_insert(0);
                    *counter += 1;

                    match *val{ //合格者の試験時偏差値を集計
                        Config::R_ADMISSION_1ST | Config::R_ADMISSION_2ND |
                        Config::R_ADMISSION_3RD | Config::R_ADMISSION_RSV => {
                            new_dev += *students[*col].exam_dev(x.index) as f64 / 1000.0;
                            // new_dev += students[*col].score as f64 / 1000.0;
                        },
                        _ => (),
                    }
                    Some(*val)
                }else{
                    None
                }
            }).collect::<Vec<Option<u8>>>();
        
        //件数集計
        //一次合格者数
        let enroll_1st_count = count(&values, Config::ENROLL_1ST) + 
                           count(&values, Config::ENROLL_2ND);
         //追加合格者数
        let enroll_add_count = count(&values, Config::ENROLL_3RD);

        //一次合格入学者数
        let admission_1st_count = count_eq(&counters, &Config::R_ADMISSION_1ST) +
                              count_eq(&counters, &Config::R_ADMISSION_2ND);

         //一次合格保留後入学者数
        let admission_rsv_count = count_eq(&counters, &Config::R_ADMISSION_RSV);

        //追加合格入学者数
        let admission_add_count = count_eq(&counters, &Config::R_ADMISSION_3RD);
        
        //入学金納付のみ者数
        let paid_only_count = count_eq(&counters, &Config::R_DECLINE1_PAID);

        //入学者総数
        let admissons_all = admission_1st_count + admission_rsv_count + admission_add_count;

        wtr.serialize(CollegeResult {
            index: x.index, //偏差値昇順ソート後の連番。配列のインデックス
            cid: x.cid, //旺文社の大学番号
            name: x.name.clone(),  //  大学名
            institute: x.institute, // 設置区分：1国立 2公立 3私立
            dev: x.dev, // 偏差値
            enroll: x.enroll, //　入学定員数
            over_rate: x.over_rate, //合格者超過率
        
            //シミュレーション結果
            apply_count: values.len() as i32, //受験者数
            decline1: count_eq(&counters, &Config::R_DECLINE1), //辞退
            decline2: count_eq(&counters, &Config::R_DECLINE2), //追加合格辞退
            enroll_1st_count: enroll_1st_count,//正規合格数
            enroll_add_count: enroll_add_count, //追加合格数
            paid_only_count: paid_only_count, //入学金納付のみ

            admisson_1st: admission_1st_count, //一次、国立合格で入学
            admisson_rsv: admission_rsv_count, //一次保留後入学
            admisson_add: admission_add_count, //追加合格入学

            admissons: admissons_all, //最終入学者数
            new_deviation: new_dev / admissons_all as f64, //入学者偏差値平均
            payments: admissons_all + paid_only_count, //入学金徴収総額
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

//ハッシュからデータを取得。キーがない時には0を返す。
fn count_eq(m: &HashMap<u8, i32>, key: &u8) -> i32{
    if let Some(val) = m.get(key){
        *val
    }else{
        0
    }
}

//bitマップ＆で一致する個数を取得する
fn count(values: &[Option<u8>], key: u8) -> i32{
    let v: Vec<u8> = values.iter().map(|x| x.unwrap())
        .filter(|x| x & key != 0).collect();
    v.len() as i32
}