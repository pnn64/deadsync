//! A song's own page and SMO's description of one chart -- the free
//! questions a rip asks before it pays for anything.
//!
//! The pack page sums a song up in one row, and the sum is no use to a rip:
//! its meters are one range over every chart of every style, pump and doubles
//! included, and its credit cell is everyone who charted any of it. What a rip
//! needs is narrower -- whether the song has each singles difficulty, and the
//! singles Challenge chart's meter and author. The song's page (`/song/<id>`)
//! lists every chart under its style's tab, each with its difficulty, meter
//! and id; `/api/chart/<id>` is the site's own reading of that one chart,
//! author included. Both are ordinary pages, not downloads, so a rip asks them
//! as freely as it reads a pack page. The chart's reading also says how long
//! the song is and how many measures the chart runs to, which a card's BPM
//! takes its colour from ([`super::bpm`]).
//!
//! Everything here blocks on the network. Worker threads only.

use deadsync_net as network;

const SONG_URL_BASE: &str = "https://stepmaniaonline.net/song/";
const CHART_URL_BASE: &str = "https://stepmaniaonline.net/api/chart/";
/// A song page is about thirty kilobytes of markup. A song with a dozen
/// styles' charts is still a fraction of this.
const MAX_SONG_PAGE_BYTES: usize = 1024 * 1024;
/// A chart's description carries every note of the chart as well, which is
/// most of its 35 to 120 KB. A marathon runs longer; past this it is not a
/// chart anybody rips for.
const MAX_CHART_BYTES: usize = 4 * 1024 * 1024;

/// The singles difficulties a rip knows, Beginner to Challenge.
pub(super) const SLOTS: usize = 5;
/// Challenge's slot: the rip's Expert chart.
pub(super) const CHALLENGE: usize = 4;

/// One chart button in the singles tab of a song's page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SongChart {
    /// The difficulty as the button prints it: `Challenge`, `Edit`.
    pub difficulty: String,
    /// 0 when the button prints no number.
    pub meter: u32,
    /// What `/api/chart/<id>` is asked about.
    pub chart_id: u64,
}

/// A song's singles charts by difficulty: the meter and chart id of each one
/// it has, the first of each where the page lists two.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Singles {
    slots: [Option<(u32, u64)>; SLOTS],
}

impl Singles {
    #[must_use]
    pub fn from_charts(charts: &[SongChart]) -> Self {
        let mut singles = Self::default();
        for chart in charts {
            let Some(slot) = slot_of(&chart.difficulty) else {
                continue;
            };
            singles.slots[slot].get_or_insert((chart.meter, chart.chart_id));
        }
        singles
    }

    /// Every one of Beginner, Easy, Medium, Hard and Challenge.
    #[must_use]
    pub fn full(&self) -> bool {
        self.slots.iter().all(Option::is_some)
    }

    /// The Challenge chart's meter and id: the rip's Expert chart.
    #[must_use]
    pub fn expert(&self) -> Option<(u32, u64)> {
        self.slots[CHALLENGE]
    }
}

/// Which of the five a difficulty name is, by the relay's table: the names
/// StepMania uses, and the older ones a `.sm` may still carry. An `Edit` is
/// none of them.
#[must_use]
pub(super) fn slot_of(difficulty: &str) -> Option<usize> {
    match difficulty.trim().to_ascii_lowercase().as_str() {
        "beginner" => Some(0),
        "easy" | "basic" | "light" => Some(1),
        "medium" | "another" | "trick" | "standard" | "difficult" => Some(2),
        "hard" | "ssr" | "maniac" | "heavy" => Some(3),
        "challenge" | "expert" | "oni" | "smaniac" => Some(CHALLENGE),
        _ => None,
    }
}

/// The singles charts a song's page lists, in the page's order.
pub(super) fn fetch_song_charts(
    agent: &network::HttpAgent,
    song_id: u64,
) -> Result<Vec<SongChart>, String> {
    let body = fetch_text(
        agent,
        format!("{SONG_URL_BASE}{song_id}"),
        MAX_SONG_PAGE_BYTES,
    )?;
    Ok(parse_song_page(&body))
}

/// The chart buttons of the `dance-single` tab. Every style has a tab of its
/// own, built the same way -- a pump chart's `Challenge 21` looks exactly like
/// a singles one -- so nothing outside that one tab is read.
#[must_use]
pub(super) fn parse_song_page(html: &str) -> Vec<SongChart> {
    let Some(pane) = singles_pane(html) else {
        return Vec::new();
    };
    let mut charts = Vec::new();
    let mut from = 0;
    while let Some(at) = pane[from..].find("<label") {
        let start = from + at;
        let Some(close) = pane[start..].find("</label>").map(|end| start + end) else {
            break;
        };
        from = close + "</label>".len();
        let label = &pane[start..close];
        let Some(chart_id) = button_chart_id(label) else {
            continue;
        };
        let Some(text_at) = label.find('>') else {
            continue;
        };
        let text = squeeze(&label[text_at + 1..]);
        let parsed = text
            .rsplit_once(' ')
            .and_then(|(name, number)| Some((name.to_owned(), number.parse::<u32>().ok()?)));
        let (difficulty, meter) = parsed.unwrap_or((text, 0));
        charts.push(SongChart {
            difficulty,
            meter,
            chart_id,
        });
    }
    charts
}

/// The inside of the `dance-single` tab pane, up to its closing tag. Only the
/// pane itself counts: the tab's button (`dance-single-tab`) and its wrapper
/// (`dance-singleStyleContent`) carry the name too, but in other attributes.
fn singles_pane(html: &str) -> Option<&str> {
    const ID: &str = "id=\"dance-single\"";
    let mut from = 0;
    while let Some(at) = html[from..].find(ID) {
        let at = from + at;
        from = at + ID.len();
        let Some(open) = html[..at].rfind('<') else {
            continue;
        };
        let Some(end) = html[at..].find('>').map(|end| at + end) else {
            break;
        };
        if !html[open..end].contains("tab-pane") {
            continue;
        }
        let body = &html[end + 1..];
        // The pane holds the buttons and nothing nested.
        let close = body.find("</div>").unwrap_or(body.len());
        return Some(&body[..close]);
    }
    None
}

/// The chart id a button's label names: `for="button-<id>"`.
fn button_chart_id(label: &str) -> Option<u64> {
    const FOR: &str = "for=\"button-";
    let at = label.find(FOR)? + FOR.len();
    let digits = &label[at..];
    let end = digits
        .find(|ch: char| !ch.is_ascii_digit())
        .unwrap_or(digits.len());
    digits[..end].parse().ok()
}

/// Tags dropped and whitespace squeezed: what the button says.
fn squeeze(fragment: &str) -> String {
    let mut text = String::with_capacity(fragment.len());
    let mut in_tag = false;
    for ch in fragment.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if in_tag => {}
            _ => text.push(ch),
        }
    }
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// What SMO says about one chart. The site's own parse of the simfile, so
/// for a `.sm` the author is the chart's description field, as StepMania
/// reads it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct ChartFacts {
    /// May be empty: plenty of charts credit nobody.
    pub author: String,
    pub meter: u32,
    pub difficulty: String,
    pub charttype: String,
    /// The site's hash of the chart's notes, the same in every pack that
    /// carries it.
    pub chart_key: String,
    /// How long the song plays, as the site measured it.
    pub length_ms: Option<u32>,
    /// How many measures the chart's notes run to.
    pub measures: Option<u32>,
}

pub(super) fn fetch_chart(agent: &network::HttpAgent, chart_id: u64) -> Result<ChartFacts, String> {
    let body = fetch_text(
        agent,
        format!("{CHART_URL_BASE}{chart_id}"),
        MAX_CHART_BYTES,
    )?;
    parse_chart(&body)
}

/// The fields a rip reads. The rest -- the notes above all, `chartdata` --
/// is skipped over unread.
#[derive(serde::Deserialize)]
struct ChartBody {
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    meter: Option<serde_json::Value>,
    #[serde(default)]
    difficulty: Option<String>,
    #[serde(default)]
    charttype: Option<String>,
    #[serde(default)]
    chart_key: Option<String>,
}

pub(super) fn parse_chart(body: &str) -> Result<ChartFacts, String> {
    let chart: ChartBody =
        serde_json::from_str(body).map_err(|error| format!("chart response: {error}"))?;
    let meter = match chart.meter {
        Some(serde_json::Value::Number(number)) => number
            .as_u64()
            .and_then(|meter| u32::try_from(meter).ok())
            .unwrap_or(0),
        Some(serde_json::Value::String(text)) => text.trim().parse().unwrap_or(0),
        _ => 0,
    };
    let (length_ms, measures) = lengths(body);
    Ok(ChartFacts {
        author: chart.author.unwrap_or_default().trim().to_owned(),
        meter,
        difficulty: chart.difficulty.unwrap_or_default(),
        charttype: chart.charttype.unwrap_or_default(),
        chart_key: chart.chart_key.unwrap_or_default(),
        length_ms,
        measures,
    })
}

/// The song's length and the chart's, for the tempo a card colours its BPM
/// by. Read apart from what the rip decides on: a reply that has these in
/// some other shape loses only the colour's average, never the song.
#[derive(serde::Deserialize)]
struct Lengths {
    #[serde(default)]
    song: Option<SongLength>,
    /// The notes, one entry a measure, each read only to be counted.
    #[serde(default)]
    chartdata: Option<Vec<serde::de::IgnoredAny>>,
}

#[derive(serde::Deserialize)]
struct SongLength {
    /// Seconds.
    #[serde(default)]
    length: Option<f64>,
}

fn lengths(body: &str) -> (Option<u32>, Option<u32>) {
    let Ok(lengths) = serde_json::from_str::<Lengths>(body) else {
        return (None, None);
    };
    let length_ms = lengths
        .song
        .and_then(|song| song.length)
        .filter(|seconds| seconds.is_finite() && *seconds > 0.0 && *seconds < 86_400.0)
        .map(|seconds| (seconds * 1000.0).round() as u32);
    let measures = lengths
        .chartdata
        .map(|measures| u32::try_from(measures.len()).unwrap_or(u32::MAX))
        .filter(|measures| *measures > 0);
    (length_ms, measures)
}

fn fetch_text(agent: &network::HttpAgent, url: String, max_bytes: usize) -> Result<String, String> {
    let response = agent
        .get(url)
        .call()
        .map_err(|error| network::error_from_ureq(error).to_string())?;
    network::read_text_body_bounded(response, max_bytes).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `/song/528511`, cut down: a singles Challenge 14, then pump tabs whose
    /// buttons look just like it.
    const ONE_SINGLES_CHART: &str = r##"
<button class="nav-link p-0 border-0 bg-transparent active"
        id="dance-single-tab"
        data-bs-toggle="tab"
        data-bs-target="#dance-single"
        aria-controls="dance-single"
        aria-selected="true">
</button>
<div class="tab-content" id="dance-singleStyleContent">
    <div class="tab-pane p-0 fade show active" id="dance-single" data-toggle="buttons" role="tablist" aria-labelledby="dance-single-tab">
    <!-- Chart buttons -->
        <input
            type="radio"
            class="btn-check"
            name="btnradio"
            data-id="#1377551"
            data-chart-id="1377551"
            role="chart"
            id="button-1377551"
            autocomplete="off"
            onclick="loadChart('1377551')"
            checked>
        <label class="
            btn btn-sm btn-outline-primary
            " for="button-1377551">Challenge 14</label>
    </div>
</div>
<div class="tab-content" id="pump-singleStyleContent">
    <div class="tab-pane p-0 fade show " id="pump-single" data-toggle="buttons" role="tablist" aria-labelledby="pump-single-tab">
        <input type="radio" class="btn-check" name="btnradio" data-id="#1377546"
            data-chart-id="1377546" role="chart" id="button-1377546">
        <label class="
            btn btn-sm btn-outline-danger
            " for="button-1377546">Hard 18</label>
        <input type="radio" class="btn-check" name="btnradio" data-id="#1377547"
            data-chart-id="1377547" role="chart" id="button-1377547">
        <label class="
            btn btn-sm btn-outline-primary
            " for="button-1377547">Challenge 21</label>
    </div>
</div>
"##;

    /// `/song/526744`, cut down: all five singles charts, then a doubles tab.
    const FULL_SINGLES: &str = r##"
<div class="tab-content" id="dance-singleStyleContent">
    <div class="tab-pane p-0 fade show active" id="dance-single" data-toggle="buttons" role="tablist" aria-labelledby="dance-single-tab">
    <!-- Chart buttons -->
        <input type="radio" class="btn-check" name="btnradio" data-chart-id="1373630" id="button-1373630" checked>
        <label class="
            btn btn-sm btn-outline-info" for="button-1373630">Beginner 1</label>
        <input type="radio" class="btn-check" name="btnradio" data-chart-id="1373629" id="button-1373629">
        <label class="
            btn btn-sm btn-outline-success
            " for="button-1373629">Easy 4</label>
        <input type="radio" class="btn-check" name="btnradio" data-chart-id="1373628" id="button-1373628">
        <label class="
            btn btn-sm btn-outline-warning
            " for="button-1373628">Medium 6</label>
        <input type="radio" class="btn-check" name="btnradio" data-chart-id="1373627" id="button-1373627">
        <label class="
            btn btn-sm btn-outline-danger
            " for="button-1373627">Hard 8</label>
        <input type="radio" class="btn-check" name="btnradio" data-chart-id="1373626" id="button-1373626">
        <label class="
            btn btn-sm btn-outline-primary
            " for="button-1373626">Challenge 10</label>
    </div>
</div>
<div class="tab-content" id="dance-doubleStyleContent">
    <div class="tab-pane p-0 fade show " id="dance-double" data-toggle="buttons" role="tablist" aria-labelledby="dance-double-tab">
        <input type="radio" class="btn-check" name="btnradio" data-chart-id="1373631" id="button-1373631">
        <label class="
            btn btn-sm btn-outline-primary
            " for="button-1373631">Challenge 10</label>
    </div>
</div>
"##;

    /// `/api/chart/485676`, with its notes cut to one measure.
    const CHART: &str = r#"{"chart_id": 485676, "author": "Mad Matt", "npsgraph": "[0.0, 2.0, 7.5]", "npsavg": 10.4374978125, "npspeak": 19.000126, "meter": 21, "charttype": "dance-single", "difficulty": "Challenge", "taps": {"total": 1524, "taps": 1524, "jumps": 0, "holds": 9, "mines": 0, "hands": 0, "rolls": 0, "lifts": 0, "fakes": 0}, "chart_key": "X31013383114125e64aa270738ed05377c1dadd94", "song": {"id": 172169, "name": "Almighty Power of the Cosmos", "artist": "X Kirby", "bpm": "185", "length": 156.733994}, "chartdata": [[0, [["0", "0", "0", "0"], ["0", "0", "0", "0"], ["0", "0", "0", "0"], ["0", "0", "0", "0"]]]], "chartwidth": 128, "common_packs": [{"id": 328, "name": "Bearpocalypse 4 - Tiers", "size": 111076322}, {"id": 3799, "name": "Tachyon Epsilon", "size": 98598990}]}"#;

    /// `/api/chart/471997`: a chart that credits nobody.
    const UNCREDITED_CHART: &str = r##"{"chart_id": 471997, "author": "", "meter": 21, "charttype": "dance-single", "difficulty": "Challenge", "chart_key": "X1421a28e70a3aff21546a659e3f45c12b380014b", "song": {"id": 165154, "name": "Akari來了哦!", "artist": "ナナヲアカリ"}, "chartdata": [], "common_packs": [{"id": 5262, "name": "#1 Extra-Terrestrial Secret Training Box", "size": 194849003}]}"##;

    #[test]
    fn only_the_singles_tab_is_read() {
        let charts = parse_song_page(ONE_SINGLES_CHART);
        assert_eq!(
            charts,
            vec![SongChart {
                difficulty: "Challenge".to_owned(),
                meter: 14,
                chart_id: 1_377_551,
            }],
            "the pump tab's Hard 18 and Challenge 21 are not singles charts"
        );
        let singles = Singles::from_charts(&charts);
        assert_eq!(singles.expert(), Some((14, 1_377_551)));
        assert!(!singles.full());
    }

    #[test]
    fn five_singles_charts_make_a_full_song() {
        let charts = parse_song_page(FULL_SINGLES);
        let labels: Vec<(&str, u32)> = charts
            .iter()
            .map(|chart| (chart.difficulty.as_str(), chart.meter))
            .collect();
        assert_eq!(
            labels,
            [
                ("Beginner", 1),
                ("Easy", 4),
                ("Medium", 6),
                ("Hard", 8),
                ("Challenge", 10)
            ]
        );
        let singles = Singles::from_charts(&charts);
        assert!(singles.full());
        assert_eq!(
            singles.expert(),
            Some((10, 1_373_626)),
            "the singles Challenge, not the doubles one"
        );
    }

    #[test]
    fn a_page_without_a_singles_tab_has_no_charts() {
        let pump_only = ONE_SINGLES_CHART.replace("id=\"dance-single\"", "id=\"dance-solo\"");
        assert!(parse_song_page(&pump_only).is_empty());
        assert!(parse_song_page("<html></html>").is_empty());
    }

    #[test]
    fn slots_follow_the_relays_names_and_an_edit_has_none() {
        assert_eq!(slot_of("Beginner"), Some(0));
        assert_eq!(slot_of(" basic "), Some(1));
        assert_eq!(slot_of("Trick"), Some(2));
        assert_eq!(slot_of("MANIAC"), Some(3));
        assert_eq!(slot_of("Oni"), Some(CHALLENGE));
        assert_eq!(slot_of("Expert"), Some(CHALLENGE));
        assert_eq!(slot_of("Edit"), None);

        let charts = [
            SongChart {
                difficulty: "Edit".to_owned(),
                meter: 13,
                chart_id: 1,
            },
            SongChart {
                difficulty: "Challenge".to_owned(),
                meter: 12,
                chart_id: 2,
            },
            SongChart {
                difficulty: "Challenge".to_owned(),
                meter: 15,
                chart_id: 3,
            },
        ];
        assert_eq!(
            Singles::from_charts(&charts).expert(),
            Some((12, 2)),
            "the first Challenge the page lists"
        );
    }

    #[test]
    fn a_chart_is_read_without_its_notes() {
        let chart = parse_chart(CHART).unwrap();
        assert_eq!(
            chart,
            ChartFacts {
                author: "Mad Matt".to_owned(),
                meter: 21,
                difficulty: "Challenge".to_owned(),
                charttype: "dance-single".to_owned(),
                chart_key: "X31013383114125e64aa270738ed05377c1dadd94".to_owned(),
                length_ms: Some(156_734),
                measures: Some(1),
            }
        );
    }

    #[test]
    fn lengths_that_are_not_there_or_not_numbers_are_none() {
        let chart = parse_chart(UNCREDITED_CHART).unwrap();
        assert_eq!(chart.length_ms, None, "a song with no length");
        assert_eq!(chart.measures, None, "a chart with no notes");
        let odd = parse_chart(
            r#"{"author": "Zaia", "meter": 9, "song": {"length": "2:14"}, "chartdata": "zipped"}"#,
        )
        .unwrap();
        assert_eq!(
            (odd.author.as_str(), odd.meter),
            ("Zaia", 9),
            "the chart still reads"
        );
        assert_eq!((odd.length_ms, odd.measures), (None, None));
        let unsong = parse_chart(r#"{"song": "?", "chartdata": [[0, []], [1, []]]}"#).unwrap();
        assert_eq!(
            (unsong.length_ms, unsong.measures),
            (None, None),
            "read together, or not at all"
        );
    }

    #[test]
    fn an_uncredited_chart_has_an_empty_author() {
        let chart = parse_chart(UNCREDITED_CHART).unwrap();
        assert_eq!(chart.author, "");
        assert_eq!(chart.meter, 21);
        let padded = parse_chart(r#"{"author": "  Zaia ", "meter": "9"}"#).unwrap();
        assert_eq!(padded.author, "Zaia", "trimmed");
        assert_eq!(padded.meter, 9, "a meter sent as text still reads");
        let null = parse_chart(r#"{"author": null}"#).unwrap();
        assert_eq!(null.author, "");
        assert!(parse_chart("<html>busy</html>").is_err());
    }
}
