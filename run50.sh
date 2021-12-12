#./target/release/examsim -s 1 configHS01.toml > histHS01_seed1.json
senario=5
sample=50
for ((i=1; i <= $senario; i++)); do
    for ((s=1; s <= $sample; s++)); do
        echo "シナリオ" $i "ランダムシード" $s
        ./target/release/examsim -s $s configHS0$i.toml > his$i$s.json
    done
done