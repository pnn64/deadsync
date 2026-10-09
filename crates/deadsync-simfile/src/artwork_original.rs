// Frozen from main 16a06a2619a6cdcc34c647603d5dfa186e2f3e73; test visibility only.

pub(super) fn resolve_song_artwork_like_itg(
    song_dir: &Path,
    simfile_data: &[u8],
    banner_tag: &str,
    background_tag: &str,
    cdtitle_tag: &str,
    jacket_tag: &str,
) -> ResolvedSongArtwork {
    let banner = resolve_song_asset_path_like_itg(song_dir, banner_tag);
    let background = resolve_song_asset_path_like_itg(song_dir, background_tag);
    let cdtitle = resolve_song_asset_path_like_itg(song_dir, cdtitle_tag);
    let jacket = resolve_song_asset_path_like_itg(song_dir, jacket_tag);

    if banner.is_some() && background.is_some() && cdtitle.is_some() {
        return ResolvedSongArtwork {
            banner_path: banner,
            background_path: background,
            cdtitle_path: cdtitle,
        };
    }

    let [cdimage_tag, discimage_tag] = latest_simfile_tag_values(
        simfile_data,
        [b"#CDIMAGE:".as_slice(), b"#DISCIMAGE:".as_slice()],
    );
    let mut candidates = ArtworkCandidates {
        banner,
        background,
        cdtitle,
        jacket,
        cdimage: resolve_song_asset_path_like_itg(song_dir, &cdimage_tag),
        disc: resolve_song_asset_path_like_itg(song_dir, &discimage_tag),
    };
    let images = list_song_art_images(song_dir);
    fill_song_art_hints(&images, &mut candidates);

    for image in &images {
        if candidates.banner.is_some()
            && candidates.background.is_some()
            && candidates.cdtitle.is_some()
        {
            break;
        }
        if song_art_is_classified(image, &candidates) {
            continue;
        }

        let Ok((width, height)) = image_dimensions(image) else {
            continue;
        };
        if candidates.background.is_none() && width >= 320 && height >= 240 {
            candidates.background = Some(image.clone());
            continue;
        }
        if candidates.banner.is_none()
            && (100..=320).contains(&width)
            && (50..=240).contains(&height)
        {
            candidates.banner = Some(image.clone());
            continue;
        }
        if candidates.banner.is_none()
            && width > 200
            && height > 0
            && width as f32 / height as f32 > 2.0
        {
            candidates.banner = Some(image.clone());
            continue;
        }
        if candidates.cdtitle.is_none() && width <= 100 && height <= 48 {
            candidates.cdtitle = Some(image.clone());
            continue;
        }
        if candidates.jacket.is_none() && width == height {
            candidates.jacket = Some(image.clone());
            continue;
        }
        if candidates.disc.is_none()
            && width > height
            && candidates.banner.is_some()
            && !song_art_matches(image, &candidates.banner)
        {
            candidates.disc = Some(image.clone());
            continue;
        }
        if candidates.cdimage.is_none() && width == height {
            candidates.cdimage = Some(image.clone());
        }
    }

    ResolvedSongArtwork {
        banner_path: candidates.banner,
        background_path: candidates.background,
        cdtitle_path: candidates.cdtitle,
    }
}
