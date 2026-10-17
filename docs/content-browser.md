# Find Content

Find Content, on the title menu, browses stepmaniaonline.net from inside the
game: packs by tab, search, each pack's own page, and a preview of any song's
charts before anything is downloaded. Packs and single songs install straight
into the library, and Rip Packs makes booster packs of fifteen songs from all
over the site. It started as a port of the ITGmania Content Browser module.

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
| `crates/deadsync-online/src/rip_packs/` | Rip Packs: `search.rs` finds a rip's songs, `pages.rs` reads song pages and chart descriptions, `dates.rs` dates packs, songs and charts, `keep.rs` downloads a kept rip, `authors.rs` the author list, `lists.rs` fetches and caches the site's published files, and `share.rs` reports rips and keeps to the site. `mod.rs` is the runtime and the API the screen and shell use. |
| `crates/deadsync-theme-simply-love/src/screens/content_browser/rip.rs`, `rip_render.rs` | The Rip Packs tab: its options, pickers, prompts and keys, and the stage it draws. |
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
| `stepmaniaonline.net` (Rip Packs) | `/song/<id>` (a song's charts) and `/api/chart/<id>` (one chart's author and meter), besides the above |
| `api.arrowcloud.dance` | Pack popularity, for the featured strip |
| `itgdb.net` | The curated list of doubles packs |
| `itgcontent.net` | `/booster/authors.json`, `pack-dates.json` and `chart-firsts.json`, the indexes Rip Packs reads; `/api/rips` and `/api/rips/kept`, where rips are reported and marked kept (see below) |

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

**The Installed tab** lists the library. SELECT deletes a pack; START opens
its sync (`sync_dialog.rs`), which offers:

- **Measure with Null-or-Die.** The shell gathers the pack's songs and opens
  the shared pack sync review on one chart per song, of the current play style.
  The reader saves what they accept.
- **Shift ITG to NULL.** Every simfile's offsets move by
  `ITG_SYNC_OFFSET_SECONDS`. A pack already recorded as NULL is offered the
  reverse instead, NULL to ITG, recorded as ITG.
- **Record in Pack.ini only,** shown only while *Machine Options > Pack.ini
  Offsets* is on, since the engine ignores `SyncOffset` otherwise.

As the dialog opens, the shell checks the pack with the same function every
change runs again before it writes (`content_reload::writable_pack`): one
folder, inside a song root directly or in a series folder there, outside the
program's own songs, and writable. A pack that fails is shown with the reason
and cannot be changed.

Rewrites go through the same save every pack sync uses, so each simfile keeps a
`.old` copy and its cached song is reloaded in place. A simfile with no
`#OFFSET` counts as zero and is given one. The pack's `Pack.ini` then
records what it now is, and the live catalog takes the value at once, without a
rescan. That record keeps the engine from adding its own ITG correction on top,
and keeps the same shift from being made twice. `Pack.ini` is found and
edited the way the scan reads it (any case, keys inside `[Group]`), written
atomically and read back before anything is reported. A `Pack.ini` that is a
link, or not UTF-8 text, is left exactly as it is and the change is refused.

- A shift writes the record first, and restores it if no simfile could be
  changed.
- After a measure, the pack is recorded as NULL. If it was ITG -- by its own
  `Pack.ini`, or by the machine's default while Pack.ini Offsets is on -- the
  songs the review left alone (below its confidence, failed, or with no
  chart of the play style) are moved from ITG to NULL as well, so every
  song in it is what the record says.

The shift holds the frame it runs in, so the dialog draws a working panel
first and only then hands it to the shell (`take_pack_shift`).

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

## Rip Packs

A rip is fifteen songs, each from a different pack, picked at random within
the options on the tab: eras, chart authors to take or avoid, the Expert
(singles Challenge) meter range, and whether every singles difficulty must be
there. Nothing is downloaded until the player keeps the rip. The selection
rules are the ITGmania relay's, run here against stepmaniaonline.net itself.

**Finding the songs** (`search.rs`). The free pages decide first. For a
random pack, its page lists the songs; for a random song, its own page lists
its singles charts and their meters; `/api/chart/<id>` says who made the
Challenge chart. Only a song that passes every one of those costs a counted
request: its pack's zip index, which gives the song's folder, its download
size and the dates its files carry. Indexes that were paid for are kept in
`cache/rip-packs/indexes/`, keyed by the catalogue's size for the pack, so
the same pack is never paid for twice.

**Eras** (`dates.rs`). A pack is dated by the official game it rips, else
itgdb's release date, else its `Pack.ini` Year, its name and its zip's file
dates, under the ceiling of its SMO upload. A song is older than its pack
when its own files are over a year older, and its Expert chart older still
when the same chart by the same charter is in an older pack. Each of the
three must fall in a ticked era, and the card shows the chart's year. A pack
whose listing already rules it out is never opened, and a pack's `Pack.ini`
is read only when it lies in the index's tail, never with a request of its
own.

**The site's indexes** (`lists.rs`). The author list and the two date
indexes (`pack-dates.json`: each pack's upload and release dates;
`chart-firsts.json`: the packs each charter's songs are in) take a crawl of
the whole site to build, so a scheduled job builds them and the game fetches
the results from `LISTS_BASE`. Each is kept in `cache/rip-packs/` beside a
`.meta.json` holding its validators, and asked for again, conditionally, once
it is a day old, so an unchanged file costs a 304. A file that does not parse
is turned away and the copy on disk stays. With the site unreachable the copy
on disk is used however old. With no copy at all, the first search kept to
eras waits up to five seconds for one and then dates songs without it (any
other search starts at once with whatever has arrived), and the author
picker says it could not reach the site and offers a retry. A file still
missing is asked for again at most every five minutes, or at once on that
retry.

A search has budgets: eight packs at once, six hundred opened, forty seconds,
and 45 counted requests. A song holds its place in the rip before it is paid
for, so a full rip never pays for a song it then throws away. A rip that runs
out of budget is short and says so. A new rip, a reset or leaving the screen
makes a running search stale: it sends nothing more.

**Keeping** (`keep.rs`). The pack is numbered one past the larger of
`save/content-boosters.txt` and every `Content Booster Pack <n>` folder in
Songs, staged under a `._` folder the scanner skips, and renamed into place
once it is done. Songs come one at a time, each folder in one request with
the source pack's `Pack.ini` read in the same request and the `.oldsync`
backups left out. Two songs with the same folder name get `(2)`, `(3)` and
so on.

Every booster is null-synced, as single songs are. A song whose pack declares
ITG, by its own `Pack.ini` or else the catalogue, or declares nothing (taken
as ITG) has its offsets moved by `ITG_SYNC_OFFSET_SECONDS`. The booster's
`Pack.ini` says `SyncOffset=NULL` and lists where each song came from and
whether it was moved.

**The author list** (`authors.rs`). The charters credited on at least a third
of the songs of a pack they uploaded in the last two years, ranked by how many
of those packs are on arrowcloud's most-played list, then by how many they
released: the site's `authors.json`, as the site ranks it.

**Sharing** (`share.rs`). While the machine's Share Rips option is on (Online
Score Services, on by default; the shell hands it over through
`runtime_set_sharing` at startup and whenever the machine config changes),
each rip is reported to itgcontent.net once its cards are dealt:
`POST /api/rips` with what it asked for (eras, meter range, full, authors and
avoided names) and, per card, its key, pack and song ids, folder, Expert
credit and meter, year, declared sync, simfile names and size, plus `x`, the
keys a re-rip threw back. A re-rip that found nothing, or was stopped by
leaving or by a reset, sends `x` alone. The folder and simfile names are
spelled as the site's relay spells them: the zip's raw name bytes read as
UTF-8, split at `/` (`ZipEntry::site_name`), which for a zip without the
UTF-8 flag is not the `zip` crate's CP437 reading. A value the site would
refuse is sent as none, so one song cannot cost the whole report: a meter
outside 1-30, a year outside 1998 to next year (UTC), a credit over 255
characters. Titles and pack names are not sent (the site reads them from SMO
and checks every song there), and nothing about the player is. The site
answers with the rip's id, and KEEP posts `rip=<id>` to `/api/rips/kept` as
soon as the id is known, even if the rip was reset or replaced by then. Each
request runs on a thread of its own with short timeouts, is never retried,
and a failure only reaches the debug log. The requests go through a `Wire`,
which the tests replace with a recorder.

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
