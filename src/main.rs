mod college;
mod student;
mod config;

use rayon::prelude::*;
use sprs::{TriMat, CsMatBase};
use std::time::Instant;
use std::collections::HashMap;
use chrono::Local;
use anyhow::Result;
use serde_json;

use crate::college::{College, Cid, CollegeResult};
use crate::student::{Sid, Student, StudentResult};
use crate::config::Config;

pub type Matrix = CsMatBase<u8, usize, Vec<usize>, Vec<usize>, Vec<u8>, usize>;
pub type SidStatus = (usize, u8);

pub fn main() -> Result<()>{

    let begin = Instant::now();
    eprintln!("大学入試シミュレーション　Ver. 1.0");
    eprintln!("開始 {}",Local::now());

    // 設定ファイルからグローバルなConfigオブジェクトを作成
    Config::from_args()?;

    //シミュレーション実行
    run(&Config::get(), &begin)?;

    eprintln!("終了 {}",Local::now());
    eprintln!("elaspled:{:?}", begin.elapsed());
    Ok(())
}

//main loop
fn run(conf: &Config, timer: &Instant) -> Result<()>{
    //大学エージェント初期値
    let mut colleges: Vec<College> = College::from_conf(conf)?;

    for epoch in 0..conf.epochs{
        eprintln!("    epoch[{:02}]:start \t{:?}",epoch, timer.elapsed());
        match step(epoch, &mut colleges, conf){
            Ok((new_colls,college_result, student_result)) =>{
                colleges = new_colls;
                output_result(epoch, &college_result, &student_result)?;
            },
            Err(e) => eprintln!("step error epoch=[{:02}] msg=[{:?}]",epoch, e),
        }
    }

    output_history(&colleges)?;

    Ok(())
}

// シミュレーション1回分実行
fn step(epoch: i32, colleges: &mut Vec<College>, conf: &Config)
    ->Result<(Vec<College>, Vec<CollegeResult>, Vec<StudentResult>)>{
    
    //Step:0 受験生エージェントを作成
    let mut students: Vec<Student> = Student::from_conf(conf, epoch as usize);

    //国公立と私立大学に分けたベクターを用意
    let (nationals, privates) = divide_colleges(&colleges);
 
    //Step:1 出願 & 試験（学生行動）
    let apply_matrix = apply(&mut students, &nationals, &privates);
   
    //Step:2 私立一次合格発表（大学行動）
    let enroll1_matrix = enroll1(colleges, &students, &apply_matrix);

    //Step:3 入学判定１回目（学生行動）
    let adm1_matrix  = admission1(&mut students, &colleges, &enroll1_matrix);

    //Step:4 国公立合格発表（大学行動）
    let enroll2_matrix = enroll2(colleges, &students, &apply_matrix);

    //状態遷移マトリクス集計 => S x C 
    let status = &enroll1_matrix + &enroll2_matrix;
    let status = &status + &(apply_matrix.transpose_into());
    let status = &status + &(adm1_matrix.transpose_into());

    //Step:5 保留中私立合格大学への入学（学生行動）
    let adm2_matrix  = admission2(&mut students, &colleges, &status);

    //状態遷移マトリクス集計 => C x S
    let status = &adm2_matrix + &(status.transpose_into());
    
    //Step:6 私立追加合格発表（大学行動）
    let enroll3_matrix = enroll3(colleges, &students, &status);

    //状態遷移マトリクス集計 => S x C
    let status = &enroll3_matrix  + &(status.transpose_into());

    //Step:7 入学先最終決定。追加合格大学への入学（学生行動）
    let adm3_matrix  = admission3(&mut students, &colleges, &status);

    //状態遷移マトリクス集計 => C x S
    let status = &adm3_matrix + &(status.transpose_into());
    
    //シミュレーション結果を集計し、次step用大学オブジェクトと集計結果を生成
    settle(epoch, &students, &colleges, status)
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
        .reduce(|| Vec::new(), append_vector);

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
                let entries = x.enroll1(Config::get(),students, apply_mat.outer_view(idx).unwrap().indices());
                for student_idx in entries {acc.push((student_idx, idx));}
                acc
        })
        .reduce( || Vec::new(), append_vector);

    // 合格sparseマトリクス　行=受験生、列=大学、値2(合格) を作成
    make_matrix(&enroll_list, students.len(), colleges.len(), Config::ENROLL_1ST)
}

//  入学決定1回名。私立大学のみ。志望校合格時に入学 or 入学金納付のみ or パス
 fn admission1
    (students: &mut Vec<Student>, colleges: &[College], enroll_mat: &Matrix) -> Matrix { 
    let new_list: Vec<(Cid, SidStatus)> = students.par_iter_mut()
        .fold_with( Vec::new(),
            |mut acc, x|{
                let idx = x.id;
                let mut entries = x.admission1(&Config::get(), colleges, enroll_mat.outer_view(idx).unwrap().indices());
                acc.append(&mut entries);
                acc
        })
        .reduce( || Vec::new(), append_vector);

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
        .reduce( || Vec::new(), append_vector);


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
                let entries = x.enroll3(Config::get(), students, mat, idx);
                for student_idx in entries {acc.push((student_idx, idx));}
                acc
        })
        .reduce(|| Vec::new(), append_vector);

    // 合格sparseマトリクス　行=受験生、列=大学、値32(追加合格) を作成
    make_matrix(&new_list, students.len(), colleges.len(), Config::ENROLL_3RD)
}


// 保留中大学への入学
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
        .reduce( || Vec::new(), append_vector);

    // 入学者sparseマトリクス　行=大学、列=受験生、値32(保留中私立入学) を作成
    make_matrix(&admission_list, colleges.len(), students.len(), Config::ADMISSION_2ND)
}

// 追加合格大学への入学
fn admission3
    (students: &mut Vec<Student>, colleges: &[College], mat: &Matrix) -> Matrix { 
    let admission_list: Vec<(Cid, Sid)> = students.par_iter_mut()
        .fold_with( Vec::new(),
            |mut acc, x|{
                let idx = x.id;
                if let Some(college_idx) = x.admission3(&Config::get(), colleges, mat, idx){
                    acc.push((college_idx, idx));
                }
                acc
        })
        .reduce( || Vec::new(), append_vector);

    // 入学者sparseマトリクス　行=大学、列=受験生、値64(最終決定入学先) を作成
    make_matrix(&admission_list, colleges.len(), students.len(), Config::ADMISSION_3RD)
}

fn append_vector<T>(mut left: Vec<T>, mut right: Vec<T>) -> Vec<T>{
    left.append(&mut right);
    left
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


//シミュレーション結果を集計し、次step用大学オブジェクトと集計結果を生成
fn settle(epoch: i32, students: &Vec<Student>, colleges: &Vec<College>, status: Matrix)
    ->Result<(Vec<College>, Vec<CollegeResult>, Vec<StudentResult>)>{

    let mut new_colleges: Vec<College> = Vec::new();
    let mut college_results: Vec<CollegeResult> = Vec::new();
    let mut student_map = HashMap::new();

    for x in colleges {
        let mut new_dev: f64 = 0.0; //入学者の偏差値合計
        let mut counters = HashMap::new();
        let values = status.outer_view(x.index).unwrap().indices().iter()
            .map(|col|{
                if let Some(val) = status.get(x.index, *col){
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
                    Some( (*col,*val) )
                }else{
                    None
                }
            }).collect::<Vec<Option<(Sid, u8)>>>();

        //
        // //学生別ログ出力用ハッシュマップ作成。key=Sid, value=Vec<(Cid,status)>
        values.iter().for_each(|v|{
            if let Some((sid, val)) = v{
                let c_vec = student_map.entry(*sid).or_insert(Vec::new());
                c_vec.push((x.index, *val));

            }
        });
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

        //大学集計結果オブジェクト作成
        let college_result = CollegeResult{
            epoch: epoch,
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
        };

        //次エポック用大学エージェント作成
        new_colleges.push(x.update(&college_result));
        //大学入試結果
        college_results.push(college_result);
    };

    //大学を偏差値順にソート
    new_colleges.par_sort_by(|a, b| a.score.cmp(&b.score));
    //index振り直し
    for i in 0..new_colleges.len() {new_colleges[i].index = i}

    //受験生入試結果生成
    let student_results = student::settle(epoch, students, &mut student_map, &colleges);
    
    
    Ok((new_colleges, college_results, student_results))
}


//シミュレーション結果を出力
fn output_result(epoch: i32, college_results: &[CollegeResult], student_results: &[StudentResult]) -> Result<()>{
    //大学側結果をCSVで出力
    let path = format!("{}/college{:02}.csv", Config::get().output_dir, epoch);
    let mut wtr = csv::Writer::from_path(path).unwrap();
    for c in college_results{
        wtr.serialize(c)?;
    }
    wtr.flush()?;

    //学生側結果を指定フォルダーに保存
    let path = format!("{}/student{:02}.csv", Config::get().output_dir, epoch);
    let mut wtr = csv::Writer::from_path(path).unwrap();
    for s in student_results{
        wtr.serialize(s)?;
    }
    wtr.flush()?;

    Ok(())
}

//シミュレーション結果を出力 　最終の大学エージェント（偏差値と入学定員充足率の履歴付き）をカレントに保存
fn output_history(colleges: &[College]) -> Result<()>{
    //最終の大学エージェント（偏差値と入学定員充足率の履歴付き）をカレントに保存
    //let path = "history.csv";
    let content = serde_json::to_string_pretty(&colleges).unwrap();
    println!("{}", content);
    // let mut wtr = csv::Writer::from_path(path).unwrap();
    // for c in colleges{
    //     wtr.serialize(c)?;
    // }
    // wtr.flush()?;

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
fn count(values: &[Option<(Sid, u8)>], key: u8) -> i32{
    values.iter().map(|x| x.unwrap().1)
        .filter(|x| x & key != 0)
        .count() as i32
}