use deadlib_assets::AssetManager;
use deadlib_present::actors::Actor;
use deadsync_simfile::runtime_cache::CourseData;
use deadsync_theme_simply_love::{screens, views};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone, Copy)]
struct PreviewSong {
    title: &'static str,
    subtitle: &'static str,
    artist: &'static str,
    genre: &'static str,
    bpms: &'static str,
    meter: u32,
    duration_seconds: u32,
}

const PREVIEW_SONGS: [PreviewSong; 10] = [
    PreviewSong {
        title: "After Hours",
        subtitle: "Midnight Mix",
        artist: "Neon Arcade",
        genre: "Synthwave",
        bpms: "0=126,128=158",
        meter: 12,
        duration_seconds: 126,
    },
    PreviewSong {
        title: "Glass Skyline",
        subtitle: "City Lights Edit",
        artist: "Mira Veil",
        genre: "Future Bass",
        bpms: "0=142",
        meter: 14,
        duration_seconds: 142,
    },
    PreviewSong {
        title: "Pulse Engine",
        subtitle: "Full Throttle",
        artist: "Vector Bloom",
        genre: "Techno",
        bpms: "0=160",
        meter: 16,
        duration_seconds: 163,
    },
    PreviewSong {
        title: "Velvet Circuit",
        subtitle: "Extended Mix",
        artist: "Astral Cartel",
        genre: "Nu Disco",
        bpms: "0=124,96=148",
        meter: 13,
        duration_seconds: 130,
    },
    PreviewSong {
        title: "Signal Lost",
        subtitle: "Deep Space Mix",
        artist: "Night Transit",
        genre: "Trance",
        bpms: "0=138",
        meter: 15,
        duration_seconds: 148,
    },
    PreviewSong {
        title: "Paper Satellites",
        subtitle: "Long Edit",
        artist: "Glass Harbor",
        genre: "Electro",
        bpms: "0=116",
        meter: 11,
        duration_seconds: 121,
    },
    PreviewSong {
        title: "Neon Overdrive",
        subtitle: "Turbo Mix",
        artist: "Radian",
        genre: "Hard Dance",
        bpms: "0=180",
        meter: 17,
        duration_seconds: 176,
    },
    PreviewSong {
        title: "Blue Hour",
        subtitle: "Afterdark Mix",
        artist: "Lumen",
        genre: "House",
        bpms: "0=132,128=144",
        meter: 12,
        duration_seconds: 132,
    },
    PreviewSong {
        title: "Gravity Well",
        subtitle: "Falling Mix",
        artist: "Kairo",
        genre: "Drum and Bass",
        bpms: "0=174",
        meter: 16,
        duration_seconds: 180,
    },
    PreviewSong {
        title: "Final Sequence",
        subtitle: "End Credits",
        artist: "Pixelghost",
        genre: "Breaks",
        bpms: "0=150",
        meter: 15,
        duration_seconds: 156,
    },
];

#[derive(Clone, Copy)]
struct PreviewCourse {
    group: &'static str,
    title: &'static str,
    first_song: usize,
    lives: Option<u32>,
}

const PREVIEW_COURSES: [PreviewCourse; 16] = [
    PreviewCourse {
        group: "Simply Love",
        title: "Night Drive",
        first_song: 0,
        lives: None,
    },
    PreviewCourse {
        group: "Simply Love",
        title: "Analog Dreams",
        first_song: 2,
        lives: None,
    },
    PreviewCourse {
        group: "Simply Love",
        title: "Neon Sprint",
        first_song: 4,
        lives: None,
    },
    PreviewCourse {
        group: "Simply Love",
        title: "Final Boss",
        first_song: 6,
        lives: None,
    },
    PreviewCourse {
        group: "Rhythm Lab",
        title: "Crossfade",
        first_song: 1,
        lives: None,
    },
    PreviewCourse {
        group: "Rhythm Lab",
        title: "Offbeat",
        first_song: 3,
        lives: None,
    },
    PreviewCourse {
        group: "Rhythm Lab",
        title: "Double Trouble",
        first_song: 5,
        lives: None,
    },
    PreviewCourse {
        group: "Rhythm Lab",
        title: "Stamina Test",
        first_song: 7,
        lives: Some(4),
    },
    PreviewCourse {
        group: "Endurance Series",
        title: "Long Way Home",
        first_song: 2,
        lives: None,
    },
    PreviewCourse {
        group: "Endurance Series",
        title: "Overdrive",
        first_song: 4,
        lives: None,
    },
    PreviewCourse {
        group: "Endurance Series",
        title: "After Midnight",
        first_song: 6,
        lives: None,
    },
    PreviewCourse {
        group: "Endurance Series",
        title: "Last Call",
        first_song: 8,
        lives: None,
    },
    PreviewCourse {
        group: "After Dark",
        title: "Shadow Play",
        first_song: 0,
        lives: None,
    },
    PreviewCourse {
        group: "After Dark",
        title: "Sidewinder",
        first_song: 2,
        lives: None,
    },
    PreviewCourse {
        group: "After Dark",
        title: "Off the Grid",
        first_song: 4,
        lives: None,
    },
    PreviewCourse {
        group: "After Dark",
        title: "Encore",
        first_song: 6,
        lives: None,
    },
];

const SONG_COUNT: usize = PREVIEW_SONGS.len();
const COURSE_STAGE_COUNT: usize = 6;
const SELECTED_COURSE_INDEX: usize = 0;

pub struct MenuPreviewContent {
    song_pack: deadsync_chart::SongPack,
    courses: Vec<CourseData>,
    selected_course_path: PathBuf,
    selected_course_banner_path: PathBuf,
}

impl MenuPreviewContent {
    pub fn create(directory: &Path) -> crate::export_support::Result<Self> {
        let song_pack = build_song_pack(directory)?;
        let (courses, selected_course_path, selected_course_banner_path) =
            build_courses(directory, &song_pack)?;
        Ok(Self {
            song_pack,
            courses,
            selected_course_path,
            selected_course_banner_path,
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

    pub fn course_select_actors(&self, assets: &AssetManager, selection_time: f32) -> Vec<Actor> {
        let init = views::SelectCourseInitView {
            song_packs: vec![self.song_pack.clone()],
            courses: self.courses.clone(),
            last_course_path: Some(self.selected_course_path.clone()),
            last_course_difficulty: Some("Challenge".to_owned()),
            ..Default::default()
        };
        let mut state = screens::select_course::init(init);
        state.current_banner_key = Arc::from(deadsync_assets::dynamic_media::path_texture_key(
            &self.selected_course_banner_path,
        ));
        screens::select_course::sync_runtime_view(
            &mut state,
            views::SelectCourseRuntimeView {
                players: Some([
                    views::SelectFlowPlayerView {
                        joined: true,
                        display_name: "Menu Preview".to_owned(),
                        ..Default::default()
                    },
                    views::SelectFlowPlayerView::default(),
                ]),
                score: Some(views::SelectCourseScoreView {
                    player_initials: "JIN".to_owned(),
                    player_score_percent: Some(0.9642),
                    machine_initials: Some("AAA".to_owned()),
                    machine_score_percent: Some(0.9827),
                    ..Default::default()
                }),
                ..Default::default()
            },
        );
        state.selection_animation_timer = selection_time;
        screens::select_course::get_actors(&state, assets)
    }
}

fn build_courses(
    directory: &Path,
    song_pack: &deadsync_chart::SongPack,
) -> crate::export_support::Result<(Vec<CourseData>, PathBuf, PathBuf)> {
    let courses_directory = directory.join("Courses");
    let mut courses = Vec::with_capacity(PREVIEW_COURSES.len());
    let mut selected_course_path = None;
    let mut selected_course_banner_path = None;
    for (index, course_preview) in PREVIEW_COURSES.iter().enumerate() {
        let course_directory = courses_directory.join(course_preview.group);
        std::fs::create_dir_all(&course_directory)?;
        let course_path = course_directory.join(format!("Course {:02}.crs", index + 1));
        let banner_name = format!("course-banner-{:02}.png", index + 1);
        let banner_path = course_directory.join(&banner_name);
        write_banner(&banner_path, index + SONG_COUNT)?;
        let mut text = format!(
            "#COURSE:{};\n#SCRIPTER:DeadSync Preview;\n#BANNER:{banner_name};\n",
            course_preview.title
        );
        if let Some(lives) = course_preview.lives {
            text.push_str(&format!("#LIVES:{lives};\n"));
        }
        for stage in 0..COURSE_STAGE_COUNT {
            let song_index = (course_preview.first_song + stage) % song_pack.songs.len();
            let song = &song_pack.songs[song_index];
            let song_directory = song
                .simfile_path
                .parent()
                .and_then(Path::file_name)
                .map(|name| name.to_string_lossy())
                .ok_or_else(|| std::io::Error::other("preview song has no directory name"))?;
            text.push_str(&format!(
                "#SONG:{}/{song_directory}:Challenge;\n",
                song_pack.group_name
            ));
        }
        std::fs::write(&course_path, text)?;
        let course = deadsync_simfile::course::parse_course_file(&course_path)?;
        courses.push((course_path.clone(), course));
        if index == SELECTED_COURSE_INDEX {
            selected_course_path = Some(course_path);
            selected_course_banner_path = Some(banner_path);
        }
    }

    let selected_course_path =
        selected_course_path.ok_or_else(|| std::io::Error::other("preview course not found"))?;
    let selected_course_banner_path = selected_course_banner_path
        .ok_or_else(|| std::io::Error::other("preview course banner not found"))?;
    Ok((courses, selected_course_path, selected_course_banner_path))
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
    let preview = PREVIEW_SONGS[index];
    let song_directory = pack_directory.join(format!("{:02} - {}", index + 1, preview.title));
    std::fs::create_dir_all(&song_directory)?;
    write_banner(&song_directory.join("banner.png"), index)?;
    let simfile_path = song_directory.join("preview.sm");
    write_simfile(&simfile_path, index, preview)?;
    let mut song = deadsync_simfile::app_runtime::parse_song_for_test(&simfile_path, 0.0)?;
    song.has_lua = matches!(index, 2 | 7);
    song.music_length_seconds = preview.duration_seconds as f32;
    song.total_length_seconds = preview.duration_seconds as i32;
    song.precise_last_second_seconds = preview.duration_seconds as f32;
    Ok(song)
}

fn write_banner(path: &Path, index: usize) -> crate::export_support::Result<()> {
    const PALETTE: [[u8; 3]; 10] = [
        [25, 82, 145],
        [135, 38, 87],
        [22, 107, 111],
        [153, 72, 26],
        [77, 54, 140],
        [161, 48, 96],
        [26, 113, 152],
        [45, 80, 157],
        [36, 119, 80],
        [148, 48, 51],
    ];
    let base = PALETTE[index % PALETTE.len()];
    image::RgbaImage::from_fn(512, 160, |x, y| {
        let x_ratio = x as f32 / 511.0;
        let y_ratio = y as f32 / 159.0;
        let wave = ((x_ratio * 5.0 + y_ratio * 2.0 + index as f32 * 0.7).sin() + 1.0) * 0.5;
        let shade = 0.72 + x_ratio * 0.2 + wave * 0.18;
        image::Rgba([
            (f32::from(base[0]) * shade).min(255.0) as u8,
            (f32::from(base[1]) * shade + y_ratio * 24.0).min(255.0) as u8,
            (f32::from(base[2]) * shade + (1.0 - x_ratio) * 28.0).min(255.0) as u8,
            255,
        ])
    })
    .save(path)?;
    Ok(())
}

fn write_simfile(
    path: &Path,
    index: usize,
    preview: PreviewSong,
) -> crate::export_support::Result<()> {
    let mut simfile = format!(
        "#TITLE:{};#SUBTITLE:{};#ARTIST:{};#GENRE:{};#BPMS:{};#OFFSET:0;#BANNER:banner.png;\n",
        preview.title, preview.subtitle, preview.artist, preview.genre, preview.bpms
    );
    append_chart(&mut simfile, index, preview.meter, false);
    if matches!(index, 3 | 7) {
        append_chart(&mut simfile, index, preview.meter.saturating_sub(2), true);
    }
    std::fs::write(path, simfile)?;
    Ok(())
}

fn append_chart(simfile: &mut String, index: usize, meter: u32, edit: bool) {
    let (description, difficulty) = if edit {
        ("Alternate", "Edit")
    } else {
        ("Single", "Challenge")
    };
    simfile.push_str(&format!(
        "#NOTES:dance-single:{description} {:02}:{difficulty}:{meter}:0,0,0,0,0:\n",
        index + 1
    ));
    append_note_rows(simfile, index, meter, edit);
}

fn append_note_rows(simfile: &mut String, index: usize, meter: u32, edit: bool) {
    const PHRASE: [&str; 64] = [
        "1000", "0000", "0000", "0000", "0000", "0000", "0000", "0000", "0100", "0000", "0000",
        "0000", "0000", "0000", "0000", "0000", "0010", "0000", "0000", "0000", "0000", "0000",
        "0000", "0000", "0001", "0000", "0000", "0000", "1100", "0000", "0000", "0000", "1001",
        "0000", "0000", "0000", "1110", "0000", "0000", "0000", "0200", "0000", "0000", "0000",
        "0000", "0000", "0000", "0000", "0300", "0000", "0000", "0000", "0004", "0000", "0000",
        "0000", "0003", "0000", "0000", "0000", "0M00", "0000", "00M0", "0000",
    ];
    let rows = if edit { 480 } else { 960 };
    for row in 0..rows {
        let mut notes = ['0'; 4];
        let phrase_index = row % PHRASE.len();
        let phrase = match (phrase_index, meter) {
            (28, 0..=12) => "1000",
            (32, 0..=13) => "0010",
            (36, 0..=15) => "1100",
            (52, 0..=14) => "0001",
            (56, 0..=14) => "0000",
            (60 | 62, 0..=11) => "0000",
            _ => PHRASE[phrase_index],
        }
        .as_bytes();
        for column in 0..notes.len() {
            let source_column = (column + notes.len() - index % notes.len()) % notes.len();
            notes[column] = phrase[source_column] as char;
        }
        if edit && row % 8 == 4 {
            notes[(row / 8 + index) % notes.len()] = '1';
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
        "After Hours" => (Grade::Tier04, 91.25, 3, 7),
        "Glass Skyline" => (Grade::Tier01, 99.12, 1, 3),
        "Signal Lost" => (Grade::Tier02, 97.08, 2, 4),
        _ => return None,
    };
    Some(deadsync_score::CachedScore {
        grade,
        score_percent,
        lamp_index: Some(lamp_index),
        lamp_judge_count: Some(lamp_judge_count),
    })
}
