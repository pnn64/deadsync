use super::*;

#[expect(
    dead_code,
    reason = "frozen overlay fields preserve the baseline layout"
)]
#[derive(Clone, Debug)]
pub struct SrpgShopOverlayStateData {
    pub(super) side: PlayerSide,
    pub(super) shop_index: usize,
    pub(super) item_indices: [usize; 4],
    pub(super) queued: HashSet<String>,
    pub(super) confirm: Option<PurchaseConfirm>,
    pub(super) local_message: Option<String>,
    pub(super) presentation_revision: u64,
    pub(super) presentation: RefCell<Option<SrpgShopPresentation>>,
}

pub(super) fn queue_key(shop_id: u32, item: &SrpgShopItem) -> String {
    format!("{shop_id}:{}", item.item_id)
}

pub(super) fn ready_count(overlay: &SrpgShopOverlayStateData, shop: &SrpgShop) -> usize {
    shop.items
        .iter()
        .filter(|item| {
            item.owned
                && item.download_url.is_some()
                && !item.downloaded
                && !overlay.queued.contains(&queue_key(shop.id, item))
        })
        .count()
}

pub(super) fn download_all(
    overlay: &mut SrpgShopOverlayStateData,
    shop: &SrpgShop,
) -> SrpgShopInputOutcome {
    let downloads = shop
        .items
        .iter()
        .filter(|item| item.owned && !item.downloaded)
        .filter_map(|item| {
            let url = item.download_url.as_ref()?;
            overlay
                .queued
                .insert(queue_key(shop.id, item))
                .then(|| SrpgShopDownload {
                    name: item.name.clone(),
                    url: url.clone(),
                })
        })
        .collect::<Vec<_>>();
    if downloads.is_empty() {
        overlay.local_message = Some("All owned songs are downloaded or queued.".to_string());
        return SrpgShopInputOutcome::ChangedSelection;
    }
    overlay.local_message = Some(format!("Queued {} songs for download.", downloads.len()));
    SrpgShopInputOutcome::DownloadAll {
        shop_id: shop.id,
        downloads,
    }
}

pub(super) fn active_shop<'a>(
    overlay: &SrpgShopOverlayStateData,
    snapshot: &'a SrpgShopSnapshot,
) -> Option<&'a SrpgShop> {
    let shop_id = SRPG_SHOP_IDS[overlay.shop_index];
    snapshot.shops.iter().find(|shop| shop.id == shop_id)
}

pub(super) fn bulk_row_detail(overlay: &SrpgShopOverlayStateData, shop: &SrpgShop) -> String {
    let ready = ready_count(overlay, shop);
    let downloaded = shop
        .items
        .iter()
        .filter(|item| item.owned && item.downloaded)
        .count();
    format!("{ready} READY  •  {downloaded} DOWNLOADED")
}

pub(super) fn push_bulk_detail(
    actors: &mut Vec<Actor>,
    overlay: &SrpgShopOverlayStateData,
    shop: &SrpgShop,
    meta: ShopMeta,
    cx: f32,
    cy: f32,
    bold_font: &'static str,
) {
    let x = PANEL_W.mul_add(-0.5, cx) + 15.0;
    let ready = ready_count(overlay, shop);
    let downloaded = shop
        .items
        .iter()
        .filter(|item| item.owned && item.downloaded)
        .count();
    let message = overlay.local_message.clone().unwrap_or_else(|| {
        if ready == 0 {
            "All owned songs are downloaded or queued.".to_string()
        } else {
            format!("Press START to download all {ready} ready songs.")
        }
    });
    actors.push(act!(quad:
        align(0.0, 0.0): xy(x - 5.0, cy - 115.0): zoomto(286.0, 250.0):
        diffuse(0.0, 0.0, 0.0, 0.72): z(Z + 4)
    ));
    actors.push(act!(text:
        font(bold_font): settext("DOWNLOAD ALL SONGS"):
        align(0.0, 0.5): xy(x + 4.0, cy - 99.0): zoom(0.34): maxwidth(260.0):
        diffuse(1.0, 1.0, 1.0, 1.0): z(Z + 6): horizalign(left)
    ));
    actors.push(act!(text:
        font("miso"): settext(format!("{ready} READY  •  {downloaded} DOWNLOADED")):
        align(0.0, 0.0): xy(x + 4.0, cy - 78.0): zoom(0.72): maxwidth(260.0):
        diffuse(meta.tint[0], meta.tint[1], meta.tint[2], 1.0): z(Z + 6): horizalign(left)
    ));
    actors.push(act!(text:
        font("miso"): settext("Queue every owned song in this shop that is not already present in the selected shop folder."):
        align(0.0, 0.0): xy(x + 4.0, cy - 35.0): zoom(0.66):
        wrapwidthpixels(395.0): maxwidth(260.0):
        diffuse(0.92, 0.92, 0.92, 1.0): z(Z + 6): horizalign(left)
    ));
    actors.push(act!(text:
        font(bold_font): settext(message):
        align(0.0, 1.0): xy(x + 4.0, cy + 123.0): zoom(0.25): maxwidth(264.0):
        diffuse(meta.tint[0], meta.tint[1], meta.tint[2], 1.0): z(Z + 6): horizalign(left)
    ));
}

pub(super) fn push_item_detail(
    actors: &mut Vec<Actor>,
    overlay: &SrpgShopOverlayStateData,
    shop: &SrpgShop,
    item: &SrpgShopItem,
    meta: ShopMeta,
    cx: f32,
    cy: f32,
    bold_font: &'static str,
) {
    let x = PANEL_W.mul_add(-0.5, cx) + 15.0;
    actors.push(act!(quad:
        align(0.0, 0.0): xy(x - 5.0, cy - 115.0): zoomto(286.0, 250.0):
        diffuse(0.0, 0.0, 0.0, 0.72): z(Z + 4)
    ));
    actors.push(act!(text:
        font(bold_font): settext(item.name.clone()):
        align(0.0, 0.5): xy(x + 4.0, cy - 99.0): zoom(0.34): maxwidth(260.0):
        diffuse(1.0, 1.0, 1.0, 1.0): z(Z + 6): horizalign(left)
    ));
    actors.push(act!(text:
        font("miso"): settext(item.effect.clone()): align(0.0, 0.0):
        xy(x + 4.0, cy - 78.0): zoom(0.72): wrapwidthpixels(350.0): maxwidth(260.0):
        diffuse(meta.tint[0], meta.tint[1], meta.tint[2], 1.0): z(Z + 6): horizalign(left)
    ));
    actors.push(act!(text:
        font("miso"): settext(item.description.clone()): align(0.0, 0.0):
        xy(x + 4.0, cy - 35.0): zoom(0.66): wrapwidthpixels(395.0): maxwidth(260.0):
        diffuse(0.92, 0.92, 0.92, 1.0): z(Z + 6): horizalign(left)
    ));
    let message = overlay.local_message.clone().or_else(|| {
        active_message(
            item,
            shop.balance,
            meta.currency,
            overlay.queued.contains(&queue_key(shop.id, item)),
        )
    });
    actors.push(act!(text:
        font(bold_font): settext(message.unwrap_or_default()):
        align(0.0, 1.0): xy(x + 4.0, cy + 123.0): zoom(0.25): maxwidth(264.0):
        diffuse(meta.tint[0], meta.tint[1], meta.tint[2], 1.0): z(Z + 6): horizalign(left)
    ));
}

pub(super) fn push_catalog(
    actors: &mut Vec<Actor>,
    overlay: &SrpgShopOverlayStateData,
    snapshot: &SrpgShopSnapshot,
    meta: ShopMeta,
    cx: f32,
    cy: f32,
    bold_font: &'static str,
) {
    let Some(shop) = active_shop(overlay, snapshot) else {
        push_status(
            actors,
            "This shop is unavailable.",
            [1.0, 0.5, 0.4, 1.0],
            cx,
            cy,
            bold_font,
        );
        return;
    };
    if shop.items.is_empty() {
        push_status(
            actors,
            "Nothing is currently listed here.",
            [1.0; 4],
            cx,
            cy,
            bold_font,
        );
        return;
    }
    let row_count = shop.items.len() + 1;
    let selected = overlay.item_indices[overlay.shop_index].min(row_count - 1);
    let start = selected
        .saturating_sub(VIEW_ROWS / 2)
        .min(row_count.saturating_sub(VIEW_ROWS));
    let visible_rows = row_count.min(VIEW_ROWS);
    let list_h = LIST_MARGIN.mul_add(2.0, ((visible_rows - 1) as f32).mul_add(ROW_H, ROW_H - 3.0));
    let list_y = ((visible_rows - 1) as f32 * ROW_H).mul_add(0.5, LIST_Y);
    actors.push(act!(quad:
        align(0.5, 0.5): xy(cx + LIST_X, cy + list_y): zoomto(LIST_W, list_h):
        diffuse(0.0, 0.0, 0.0, 0.78): z(Z + 4)
    ));
    for (slot, row_index) in (start..row_count.min(start + VIEW_ROWS)).enumerate() {
        let y = (slot as f32).mul_add(ROW_H, cy + LIST_Y);
        let active = row_index == selected;
        let (name, detail) = if row_index == 0 {
            (
                "DOWNLOAD ALL SONGS".to_string(),
                bulk_row_detail(overlay, shop),
            )
        } else {
            let item = &shop.items[row_index - 1];
            (
                item.name.clone(),
                item_row_detail(
                    item,
                    meta.currency,
                    overlay.queued.contains(&queue_key(shop.id, item)),
                ),
            )
        };
        actors.push(act!(quad:
            align(0.5, 0.5): xy(cx + LIST_X, y): zoomto(LIST_W - 8.0, ROW_H - 3.0):
            diffuse(meta.tint[0], meta.tint[1], meta.tint[2], if active { 0.82 } else { 0.12 }):
            z(Z + 5)
        ));
        actors.push(act!(text:
            font(bold_font): settext(name):
            align(0.0, 0.5): xy(LIST_W.mul_add(-0.5, cx + LIST_X) + 9.0, y - 5.0): zoom(0.25):
            maxwidth(205.0): diffuse(1.0, 1.0, 1.0, if active { 1.0 } else { 0.76 }):
            z(Z + 6): horizalign(left)
        ));
        actors.push(act!(text:
            font("miso"): settext(detail):
            align(0.0, 0.5): xy(LIST_W.mul_add(-0.5, cx + LIST_X) + 9.0, y + 8.0): zoom(0.67):
            maxwidth(255.0): diffuse(0.88, 0.88, 0.88, if active { 1.0 } else { 0.62 }):
            z(Z + 6): horizalign(left)
        ));
    }
    if selected == 0 {
        push_bulk_detail(actors, overlay, shop, meta, cx, cy, bold_font);
    } else {
        push_item_detail(
            actors,
            overlay,
            shop,
            &shop.items[selected - 1],
            meta,
            cx,
            cy,
            bold_font,
        );
    }
}
