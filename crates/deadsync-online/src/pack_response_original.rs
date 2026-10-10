// Frozen from 361286535d5458eead4c0fa73f4f345559aea936; unchanged helpers/types are shared.
use super::*;

pub(super) fn numbers(list: &str) -> Vec<u32> {
    let mut out = Vec::new();
    for piece in list.split(',') {
        let digits: String = piece.chars().filter(char::is_ascii_digit).collect();
        if let Ok(value) = digits.parse::<u32>() {
            out.push(value);
        }
    }
    out
}

pub(super) fn parse(html: &str) -> PackPage {
    let mut page = PackPage {
        banner_url: og_banner(html),
        ..PackPage::default()
    };

    // The Chart.js block: the meters present, and how many charts at each.
    // `data:` is looked for after `labels:` rather than from the top, so a
    // stray `data:` URI earlier in the document cannot be read as the counts.
    if let Some((labels, after_labels)) = between(html, 0, "labels:", "]")
        && let Some((counts, _)) = between(html, after_labels, "data:", "]")
    {
        let labels = numbers(labels);
        let counts = numbers(counts);
        for (index, meter) in labels.iter().enumerate() {
            if (1..=METER_MAX).contains(meter) {
                page.meter_labels.push(*meter);
                page.meter_counts
                    .push(counts.get(index).copied().unwrap_or(0));
            }
        }
    }

    page.songs = parse_songs(html);

    let mut authors: Vec<String> = Vec::new();
    for song in &page.songs {
        for name in song.credit.split(',') {
            let name = name.trim();
            if !name.is_empty() && !authors.iter().any(|known| known == name) {
                authors.push(name.to_owned());
            }
        }
    }
    page.authors = authors;
    let mut styles: Vec<String> = Vec::new();
    for song in &page.songs {
        for style in &song.styles {
            if !styles.iter().any(|known| known == style) {
                styles.push(style.clone());
            }
        }
    }
    page.styles = styles;
    // The site's own total, which counts charts the histogram drops as jokes.
    // Summing the bars is the fallback, not the answer.
    page.chart_count = stat(html, "Charts").or_else(|| {
        page.meter_counts
            .iter()
            .copied()
            .reduce(u32::saturating_add)
            .map(|total| total.to_string())
    });
    page
}

pub(super) fn parse_songs(html: &str) -> Vec<SongRow> {
    let mut songs = Vec::new();
    let mut at = 0usize;
    while let Some((row, next)) = between(html, at, "<tr>", "</tr>") {
        at = next;
        if !row.contains("/song/") {
            continue;
        }
        let cells = table_cells(row);
        // The site prints eight columns; a row with fewer is a header or a
        // layout row rather than a song.
        if cells.len() < 8 {
            continue;
        }
        let credit = clean(
            cells[5]
                .replace("<br>", ", ")
                .replace("<br/>", ", ")
                .replace("<br />", ", ")
                .as_str(),
        );
        let song = SongRow {
            title: clean(title_of(cells[1].as_str()).unwrap_or_default().as_str()),
            artist: clean(artist_of(cells[1].as_str()).unwrap_or_default().as_str()),
            length: clean(cells[3].as_str()),
            bpm: clean(cells[4].as_str()),
            credit: credit.trim_end_matches([',', ' ']).to_owned(),
            meters: clean(cells[7].as_str()),
            image_url: attribute(cells[0].as_str(), "src").and_then(media_url),
            styles: attribute(cells[6].as_str(), "data-sort")
                .map(style_tokens)
                .unwrap_or_default(),
        };
        if song.title.is_empty() {
            continue;
        }
        songs.push(song);
    }
    songs
}

pub(super) fn table_cells(row: &str) -> Vec<String> {
    let mut cells = Vec::new();
    let mut at = 0usize;
    while let Some(open) = row.get(at..).and_then(|rest| rest.find("<td")) {
        let tag_start = at + open;
        let Some(end) = row.get(tag_start..).and_then(|rest| rest.find("</td>")) else {
            break;
        };
        let stop = tag_start + end + "</td>".len();
        cells.push(row[tag_start..stop].to_owned());
        at = stop;
    }
    cells
}

pub(super) fn title_of(cell: &str) -> Option<String> {
    let at = cell.find("/song/")?;
    let close = cell.get(at..)?.find('>')? + at + 1;
    let end = cell.get(close..)?.find("</a>")? + close;
    Some(cell[close..end].to_owned())
}

pub(super) fn artist_of(cell: &str) -> Option<String> {
    let at = cell.find("text-gray-400")?;
    let close = cell.get(at..)?.find('>')? + at + 1;
    let end = cell.get(close..)?.find("</span>")? + close;
    Some(cell[close..end].to_owned())
}
