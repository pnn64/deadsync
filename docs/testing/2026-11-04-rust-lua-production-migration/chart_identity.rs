//! Read chart identities with the production simfile parser for smoke manifests.
fn main() {
    let path = std::path::PathBuf::from(std::env::args_os().nth(1).expect("simfile"));
    let options = deadsync_simfile::song::ParseSongOptions::new(vec![], vec![], vec![]);
    // Audio duration does not participate in chart hashing. The actual app
    // decodes the pinned music file when running the resulting manifest.
    let song = deadsync_simfile::song::parse_song_meta_file(&path, &options, 0.0, |_| 0.0)
        .expect("parse chart metadata");
    for chart in song.charts {
        println!("{}\t{}\t{}", chart.chart_type, chart.difficulty, chart.short_hash);
    }
}
