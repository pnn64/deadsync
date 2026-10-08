# Find Content

Find Content, on the title menu, browses stepmaniaonline.net from inside the
game: packs by tab, search, each pack's own page, and a preview of any song's
charts before anything is downloaded. Packs and single songs install straight
into the library. It started as a port of the ITGmania Content Browser module.

This document is for working on it: where the code lives, how the pieces talk
to each other, and the rules the network code keeps.

---

## Where the code lives

| Path | What it does |
| --- | --- |
| `crates/deadsync-theme-simply-love/src/screens/content_browser/` | The screen. `state.rs` holds the screen state and the shell-facing API, `input.rs` turns keys into outcomes, `render.rs` and `layout.rs` draw it, `detail.rs` is a pack's page, `preview.rs` and `chart_window.rs` are the chart preview and the song menu. |
| `crates/deadsync-online/src/stepmaniaonline.rs` | The catalogue, the download queue and the archive install. |
| `crates/deadsync-online/src/smo_details.rs`, `smo_describe.rs` | Banners, dates and chart types for the rows on screen. |
| `crates/deadsync-online/src/pack_page.rs` | One pack's page: its songs, meters and difficulty histogram. |
| `crates/deadsync-online/src/smo_search.rs` | Search over pack names, chart credits and song titles. |
| `crates/deadsync-online/src/beginner.rs`, `popular_packs.rs`, `itgdb.rs`, `banners.rs` | The Beginner tab, the featured strip, the curated doubles list and banner images. |
| `crates/deadsync-online/src/pack_archive.rs`, `smo_songs.rs`, `song_preview.rs` | Reading single songs out of pack zips, for the preview and for single-song installs. |
| `crates/deadsync-simfile/src/song.rs` (`parse_song_bytes`) | Parses a simfile held in memory. |
| `crates/deadsync-theme-simply-love/src/screens/components/shared/noteskin_draw.rs` | Noteskin pieces drawn outside gameplay, shared with Player Options. |
| `crates/deadsync-shell/src/app/mod.rs` (`sync_content_browser_stepmaniaonline`) | The per-frame wiring between the services and the screen. |
| `crates/deadsync-shell/src/app/browser_skin.rs` | Loads the player's noteskin for the preview. |
| `crates/deadsync-shell/src/content_reload.rs` | Library rescans, deleting a pack, writing `Pack.ini`. |

---

## How it fits together

**Services.** Each online module owns its state behind one mutex and publishes
immutable `Arc` snapshots with a revision number. Requests run on worker
threads; a generation token makes a reply that arrives after its request was
replaced land nowhere. Reading a snapshot costs a lock and an `Arc` clone, so
the screen can read every service every frame.

**The shell.** Once a frame, while the browser is the current screen, the shell
gathers those snapshots into `content_browser::Services`, passes in what the
screen needs from the song library (the installed packs), and calls
`sync_stepmaniaonline`. It then drains the screen's queues: preview and
single-song requests (`take_song_requests`), music to start and stop
(`take_audio_requests`) and folders that need a rescan
(`take_pending_reload_dirs`).

**The screen.** Input becomes an `Outcome`, and outcomes become theme effects
the shell executes. The screen never touches the network, the disk or the song
cache itself. Anything it needs from them arrives through `Services` or one of
the calls above.

---

## External services

| Host | Used for |
| --- | --- |
| `stepmaniaonline.net` | `/api/packs` (the catalogue CSV), `/api/packs/datatables` (banners, dates, chart types), `/pack/<id>` (a pack's page), `/api/search` (charter and song-title search), `/download/pack/<id>/` (pack downloads, previews and single songs) |
| `api.arrowcloud.dance` | Pack popularity, for the featured strip |
| `itgdb.net` | The curated list of doubles packs |

**Every request to `/download/pack/<id>/` counts as a download of that pack on
the site, `HEAD` included.** The preview and single-song code is built around
that, and changes to it should keep it so:

- nothing is fetched except for an explicit press, and `HEAD` is never sent;
- ranges are batched into as few multi-range requests as a server accepts;
- two reads of the same pack wait for each other rather than both asking;
- a preview that has been stopped or replaced sends nothing more.

A preview costs three requests the first time a pack is opened, two for another
song from it, and one once its simfiles have been read. A single song costs one.

---

## Installing

**Packs** go through the download queue in `stepmaniaonline.rs`. The archive is
extracted under strict path rules (names stay inside the destination, no
symbolic links, no two names that would land on one file, bounded sizes) into
the Songs folder. Leaving the screen after an install offers a song reload.

**Single songs** are one song's folder read out of the pack zip and extracted
under the same rules into `Songs/Content Browser Singles - NULL Sync/<song>`.
The group gets a `Pack.ini` declaring `SyncOffset=NULL` the first time a song
lands in it. A song from a pack taken to be ITG-synced (any pack the site does
not list as null-synced) has every `#OFFSET` moved by
`ITG_SYNC_OFFSET_SECONDS` with the simfile offset writer, so it plays the same
whatever the machine's Pack.ini settings. A song already in the group is
refused before anything is downloaded.

**The Installed tab** lists the library. START sets a pack's `SyncOffset` in its
`Pack.ini`, which the engine applies only when *Machine Options > Pack.ini
Offsets* is on. SELECT deletes a pack.

---

## The chart preview

1. The pack's index is its zip's central directory, read from the end of the
   file in one request and kept for the session (four packs).
2. The song's folder is found by name, or, when folder names do not match the
   page's titles, by the titles its simfiles declare. A title the page lists
   twice is told apart by its artist.
3. The simfile is parsed with `parse_song_bytes`, which never resolves media
   paths, so a downloaded simfile cannot make the game look anything up.
   `song_preview` turns it into each chart's notes inside the sample window.
4. The charts are published first, so the window draws while the audio
   arrives. The audio is streamed into `cache/content-preview` and played by
   the shell from there.

The window draws the first joined player's noteskin, or the machine default.
`browser_skin.rs` loads it on a worker when the browser opens and uploads one
texture a frame; until it is ready the window draws plain shapes. Taps press
their receptor as they reach it; mines scroll past without pressing anything.

---

## Testing

```sh
cargo test -p deadsync-online -p deadsync-simfile
cargo test -p deadsync-theme-simply-love --lib content_browser
cargo test -p deadsync-shell --lib browser_skin
```

None of these talk to the real sites. `pack_archive` runs against a scripted
HTTP transport, and `smo_songs` installs into temporary folders. If you add a
check against the live site, keep it out of the suite and keep it to a handful
of requests, since each one counts as a download.
