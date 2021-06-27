extern crate csv;
extern crate serde;
extern crate anyhow;

use serde::Deserialize;
use anyhow::Result;
use csv::ReaderBuilder;

use crate::student::Sid;

pub type Uid = usize; //大学ID

#[derive(Debug,Clone,Deserialize)]
pub struct Univ{
    pub uid: Uid,
    pub name: String,
    pub institute: u8,
    pub pref: u8,
    pub urban: String,
    pub capa: u32,
    pub dev: f64,
    pub enroll: u32,
    pub over_rate: f64,
    #[serde(default)]
    pub s_vec: Vec<Sid>
}
impl Univ {
    pub fn from_path(path: &str) -> Result<Vec<Self>>{
        let mut univs: Vec<Univ> = Vec::new();
        let mut rdr = ReaderBuilder::new().from_path(path)?;
        for result in rdr.deserialize(){
            let univ: Self = result?;
            univs.push(univ);
        }
        Ok(univs)    
    }
    pub fn univ_scale(&self) -> usize {
        if self.capa >= 8000 { 0 }
        else if self.capa >= 4000{ 1 }
        else { 2 }
    }
    
}
