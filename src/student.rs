use crate::univ::{Univ, Uid};
// use crate::config::Config;

pub type Sid = usize; //学生ID

#[derive(Debug,Clone)]
pub struct Student{
    pub id: Sid,
    pub score: i32, //偏差値を1000倍した整数
    pub u_vec: Vec<Univ>,
    pub flg: i32,
}

impl Student {
    pub fn new(i: f64) -> Self{
        Self{id: 0,
            score: (i * 1000.0).round() as i32,
            u_vec: Vec::new(),
            flg: 0
        }
    }

    pub fn entry(&self, u: &Vec<Univ>) -> ((Sid, i32), Uid) {
        ((self.id, self.score), u[0].uid)
    }

    // pub fn print_student_num(){
    //     println!("student number {:?}", Config::get().student_number);
    // }
}

