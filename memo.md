149, 140, 132, 124, 121, 100,  93,  87,  94,  96, 102, 127
152, 143, 256, 118, 116, 100,  91,  86,  90,  89, 108, 134
151, 143, 122, 127, 128, 101,  93,  88,  91,  97, 108, 124
150, 140, 144, 128, 119, 101,  86,  84,  87,  98, 107, 133
150, 143, 138, 123, 120, 104,  91,  88,  97,  95, 111, 125


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




students[0] Student { id: 100905, score: 95371, c_vec: [], flg: 560000, rng: Xoshiro256StarStar { s: [3067308700193536694, 4573305493866406276, 2794037550489474366, 16407966112385597795] } }
students[20_000] Student { id: 80919, score: 68103, c_vec: [], flg: 560000, rng: Xoshiro256StarStar { s: [13135190836611784335, 11398814178889667318, 2442753900207682303, 16953005594989113355] } }
students[30_000] Student { id: 208069, score: 66146, c_vec: [], flg: 560000, rng: Xoshiro256StarStar { s: [2313825813703423168, 4400750755005345124, 14414088273384551795, 9842495298866273029] } }
students[56_000] Student { id: 141229, score: 62841, c_vec: [], flg: 560000, rng: Xoshiro256StarStar { s: [6178944242458974110, 17768879425055482013, 10017354295049687067, 12933785522600600534] } }

[ 713, 649, 667]
[649, 667,  713, ]

mainのデバッグプリント
    // println!("apply_entries len:{:?}", apply_list.len());
    // println!("apply_entries[56_000]={:?}", apply_list[56_000]);
    // println!("apply_entries[56_001]={:?}", apply_list[56_001]);
    // println!("apply_entries[56_002]={:?}", apply_list[56_002]);

    // let (rows, cols): (Vec<_>, Vec<_>) = apply_list.par_iter().cloned().unzip();
    // println!("rows max:{:?}", rows.par_iter().max());
    // println!("rows min:{:?}", rows.par_iter().min());
    // println!("cols max:{:?}", cols.par_iter().max());
    // println!("cols min:{:?}", cols.par_iter().min());

    // colleges.iter().for_each(|x| println!("{:?},{:?}", x.name, x.score)); 
    // println!("2015 L rate: {:?}", config::MAX_ENROLLMENT_RATES[3][colleges[0].college_scale()]);
    // println!("colleges[0] {:?}", colleges[0]);
    // println!("students[20_000] {:?}", students[20_000]);
    // println!("students[30_000] {:?}", students[30_000]);

    // println!("config sutudent num {:?}", Config::get().student_number);
    // for i in 0..10{
    //     println!("{:?}: {:?} {:?}",i, colleges[i].name, colleges[i].score);
    // }
    // println!("  lower 4:{:?} < x  <= 7:upper:{:?}",36990,37600);
    // println!("      lower_index{:?}", colleges.lower_bound_by_key(&36990, |x| x.score));
    // println!("      upper_index{:?}", colleges.upper_bound_by_key(&37600, |x| x.score));
