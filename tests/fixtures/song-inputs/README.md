# Retained song inputs

`mawaru9.tar.zst` bundles all 656 files of the authored Mawaru9 song used by
its retained semantic regression. It is an input bundle, not an accepted
full-song parity archive. Its full replay remains deferred because of its
length; the committed corpus coverage test still validates its pinned chart.

The deterministic tar starts with `manifest.json`, followed by unchanged song
files under `song/`. The manifest records each file's byte count and SHA-256;
`index.json` records the compressed bundle SHA-256. Runtime tests extract this
bundle into a temporary directory and require no external song installation.

`../song-lua/multitap-factory.lua` is Telperion's authored multitap v1.2 factory,
extracted from committed archive
`9a539969dba52899892fbfd70243f57352ea0fe8fc3330814acac77a26b8ccd9.tar.zst`,
member `song/multitap/Default.lua` (original SHA-256
`c020ecc5ef8925c28e456659037773765522c604a99e27952a391415f6a5f730`).
Only line endings were normalized in the committed Lua file. The boundary test
copies it into a temporary song, supplies generated lane data, and replaces its
Komika font name with the repository's `Common Normal` alias.
