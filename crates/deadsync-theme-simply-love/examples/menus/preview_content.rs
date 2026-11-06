use deadlib_assets::AssetManager;
use deadlib_present::actors::Actor;
use deadsync_theme_simply_love::{screens, views};
use std::{path::Path, sync::Arc};

const SONG_COUNT: usize = 10;

pub struct MenuPreviewContent {
    song_pack: deadsync_chart::SongPack,
}

impl MenuPreviewContent {
    pub fn create(directory: &Path) -> crate::export_support::Result<Self> {
        Ok(Self {
            song_pack: build_song_pack(directory)?,
        })
    }

    pub fn first_song(&self) -> Arc<deadsync_chart::SongData> {
        self.song_pack.songs[0].clone()
    }

    pub fn song_select_actors(&self, assets: &AssetManager, selection_time: f32) -> Vec<Actor> {
        let mut init = views::SelectMusicInitView::default();
        init.song_packs.push(self.song_pack.clone());
        init.policy.wheel.show_grades = true;
        init.policy.wheel.show_lamps = true;
        let play_style = init.session.play_style;
        let mut state = screens::select_music::init(init);
        let runtime = song_select_runtime(&state, play_style);
        screens::select_music::sync_runtime_view(&mut state, runtime);
        state.selection_animation_timer = selection_time;
        screens::select_music::get_actors(&state, assets, 1)
    }
}

fn build_song_pack(directory: &Path) -> crate::export_support::Result<deadsync_chart::SongPack> {
    let name = "Simply Love Menu Preview".to_owned();
    let pack_directory = directory.join(&name);
    let mut songs = Vec::with_capacity(SONG_COUNT);
    let mut banner_path = None;
    for index in 0..SONG_COUNT {
        let song = build_song(&pack_directory, index)?;
        if banner_path.is_none() {
            banner_path.clone_from(&song.banner_path);
        }
        songs.push(Arc::new(song));
    }
    Ok(deadsync_chart::SongPack {
        group_name: name.clone(),
        name,
        sort_title: "Menu Preview Pack".into(),
        translit_title: String::new(),
        series: String::new(),
        folder_series: String::new(),
        year: 2026,
        sync_pref: deadsync_chart::SyncPref::Default,
        directory: pack_directory,
        banner_path,
        songs,
    })
}

fn build_song(
    pack_directory: &Path,
    index: usize,
) -> crate::export_support::Result<deadsync_chart::SongData> {
    let title = format!("Preview Track {:02}", index + 1);
    let song_directory = pack_directory.join(format!("{:02} - {title}", index + 1));
    std::fs::create_dir_all(&song_directory)?;
    write_banner(&song_directory.join("banner.png"), index)?;
    let simfile_path = song_directory.join("preview.sm");
    write_simfile(&simfile_path, &title, index)?;
    let mut song = deadsync_simfile::app_runtime::parse_song_for_test(&simfile_path, 0.0)?;
    song.has_lua = matches!(index, 2 | 7);
    Ok(song)
}

fn write_banner(path: &Path, index: usize) -> crate::export_support::Result<()> {
    let hue = (index as u8).wrapping_mul(23);
    image::RgbaImage::from_fn(512, 160, |x, y| {
        image::Rgba([
            35_u8.saturating_add((x / 5) as u8).wrapping_add(hue),
            35_u8.saturating_add((y / 2) as u8).wrapping_add(hue / 2),
            110_u8.saturating_add((x / 8) as u8).wrapping_add(hue),
            255,
        ])
    })
    .save(path)?;
    Ok(())
}

fn write_simfile(path: &Path, title: &str, index: usize) -> crate::export_support::Result<()> {
    let bpm = 110 + index * 8;
    let mut simfile = format!(
        "#TITLE:{title};#SUBTITLE:Exporter Sample;#ARTIST:DeadSync;#BPMS:0={bpm};#OFFSET:0;#BANNER:banner.png;\n"
    );
    append_chart(&mut simfile, index, false);
    if matches!(index, 3 | 7) {
        append_chart(&mut simfile, index, true);
    }
    std::fs::write(path, simfile)?;
    Ok(())
}

fn append_chart(simfile: &mut String, index: usize, edit: bool) {
    let (description, difficulty, meter) = if edit {
        ("Menu Remix", "Edit", 6 + index)
    } else {
        ("Menu Preview", "Challenge", 8 + index)
    };
    simfile.push_str(&format!(
        "#NOTES:dance-single:{description}:{difficulty}:{meter}:0,0,0,0,0:\n"
    ));
    append_note_rows(simfile, index, edit);
}

fn append_note_rows(simfile: &mut String, index: usize, edit: bool) {
    for row in 0..64 {
        let mut notes = ['0'; 4];
        if edit && row % 3 == 0 {
            notes[(row / 3 + index + 1) % notes.len()] = '1';
        } else if !edit && row % 4 == 0 {
            notes[(row / 4 + index) % notes.len()] = '1';
        } else if !edit && row % 8 == 4 {
            notes[(row / 8 + index + 2) % notes.len()] = '1';
        }
        simfile.extend(notes);
        simfile.push('\n');
    }
    simfile.push_str(";\n");
}

fn song_select_runtime(
    state: &screens::select_music::State,
    play_style: deadsync_profile::PlayStyle,
) -> views::SelectMusicRuntimeView {
    let slots = screens::select_music::music_wheel_runtime_request(state)
        .slots
        .map(song_slot_runtime);
    views::SelectMusicRuntimeView {
        music_wheel: Some(views::MusicWheelRuntimeView {
            joined: [true, false],
            play_style,
            slots,
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn song_slot_runtime(
    request: views::MusicWheelSlotRuntimeRequest<'_>,
) -> views::MusicWheelSlotRuntimeView {
    let mut slot = views::MusicWheelSlotRuntimeView::default();
    let views::MusicWheelSlotRuntimeRequest::Song { song, has_edit, .. } = request else {
        return slot;
    };
    slot.has_edit = has_edit;
    if let Some(score) = sample_score(&song.title) {
        slot.sides[0].score = Some(score);
        slot.sides[0].lamp = score.lamp_index.map(|index| deadsync_score::CachedLamp {
            index,
            judge_count: score.lamp_judge_count,
        });
    }
    slot
}

fn sample_score(title: &str) -> Option<deadsync_score::CachedScore> {
    use deadsync_score::Grade;

    let (grade, score_percent, lamp_index, lamp_judge_count) = match title {
        "Preview Track 01" => (Grade::Tier04, 91.25, 3, 7),
        "Preview Track 02" => (Grade::Tier01, 99.12, 1, 3),
        "Preview Track 05" => (Grade::Tier02, 97.08, 2, 4),
        _ => return None,
    };
    Some(deadsync_score::CachedScore {
        grade,
        score_percent,
        lamp_index: Some(lamp_index),
        lamp_judge_count: Some(lamp_judge_count),
    })
}
