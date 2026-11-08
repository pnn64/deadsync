fn original(gameplay_song: Vec<GameplayChartData>) -> [Arc<GameplayChartData>; MAX_PLAYERS] {
    [
        Arc::new(gameplay_song[0].clone()),
        Arc::new(gameplay_song[1].clone()),
    ]
}
