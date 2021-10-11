BEGIN{
    FS=",";
    apply_count = 0;
    institute[1] = 0;
    institute[2] = 0;
    institute[3] = 0;
    print("処理開始");
}
{
    if(NR == 1){ next }
    apply_count += split($5, arr, " ");
    pattern_count[$4]++
    for(i in arr){
        split(arr[i], arr2, ":")
        institute[arr2[1]]++ #設立区分別の応募数積算
    }
}
END{
    for(i in institute){
       printf("設置区分[%d] %d\n", i, institute[i]);
    }
    print
    for(i in pattern_count){
       printf("併願区分[%d] %d\n", i, pattern_count[i]);
    }
   printf("学生数 %d\n", NR - 1);
    printf("受験数 %d\n", apply_count);
    printf("学生一人当たり平均受験数 %f\n", apply_count/(NR - 1));
}