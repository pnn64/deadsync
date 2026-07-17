use crate::act;
use crate::assets::{FontRole, machine_font_key};
use crate::config::MachineFont;
use deadlib_present::actors::Actor;
use deadlib_present::space::{screen_center_x, screen_center_y, screen_height, screen_width};
use deadsync_input::{InputEvent, VirtualAction};
use deadsync_online::srpg_shop::{
    SRPG_SHOP_IDS, SrpgShop, SrpgShopItem, SrpgShopItemKind, SrpgShopPhase, SrpgShopSnapshot,
};
use deadsync_profile::PlayerSide;
use std::collections::HashSet;

const Z: i16 = 1490;
const PANEL_W: f32 = 620.0;
const PANEL_H: f32 = 430.0;
const LIST_W: f32 = 286.0;
const LIST_X: f32 = 157.0;
const LIST_Y: f32 = -96.0;
const ROW_H: f32 = 37.0;
const VIEW_ROWS: usize = 7;
const SHOP_IMAGE_H: f32 = PANEL_W * 9.0 / 16.0;
const HEADER_H: f32 = 54.0;
const HEADER_Y: f32 = -PANEL_H * 0.5 + HEADER_H * 0.5;
const HEADER_CONTENT_Y: f32 = HEADER_Y - 8.0;
const LIST_MARGIN: f32 = 11.5;

#[derive(Clone, Copy)]
struct ShopMeta {
    name: &'static str,
    short_name: &'static str,
    currency: &'static str,
    image: &'static str,
    tint: [f32; 3],
}

const SHOPS: [ShopMeta; 4] = [
    ShopMeta {
        name: "Bronze Bistro",
        short_name: "SN",
        currency: "Bistro Bucks",
        image: "srpg10_shop/remsshopc.jpg",
        tint: [1.0, 0.40, 0.18],
    },
    ShopMeta {
        name: "Border Shop",
        short_name: "DPRT",
        currency: "Gold",
        image: "srpg10_shop/tevshopc.jpg",
        tint: [0.30, 0.84, 1.0],
    },
    ShopMeta {
        name: "Memepeace Company Store",
        short_name: "FE",
        currency: "Jej Points",
        image: "srpg10_shop/levitasshopc.jpg",
        tint: [0.75, 0.46, 1.0],
    },
    ShopMeta {
        name: "Wandering Caravan",
        short_name: "NEP",
        currency: "Wide Stamps",
        image: "srpg10_shop/janus5kshopd.jpg",
        tint: [1.0, 0.72, 0.20],
    },
];

#[derive(Clone, Debug)]
struct PurchaseConfirm {
    shop_id: u32,
    item_id: String,
    type_id: u8,
    name: String,
    cost: u64,
}

#[derive(Clone, Debug)]
pub struct SrpgShopOverlayStateData {
    side: PlayerSide,
    shop_index: usize,
    item_indices: [usize; 4],
    queued: HashSet<String>,
    confirm: Option<PurchaseConfirm>,
    local_message: Option<String>,
}

#[derive(Clone, Debug)]
pub enum SrpgShopOverlayState {
    Hidden,
    Visible(SrpgShopOverlayStateData),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SrpgShopInputOutcome {
    None,
    ChangedSelection,
    Closed,
    Refresh(PlayerSide),
    Download {
        shop_id: u32,
        name: String,
        url: String,
    },
    DownloadAll {
        shop_id: u32,
        downloads: Vec<SrpgShopDownload>,
    },
    Purchase {
        shop_id: u32,
        item_id: String,
        type_id: u8,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SrpgShopDownload {
    pub name: String,
    pub url: String,
}

pub fn show_srpg_shop_overlay(side: PlayerSide) -> SrpgShopOverlayState {
    SrpgShopOverlayState::Visible(SrpgShopOverlayStateData {
        side,
        shop_index: 0,
        item_indices: [0; 4],
        queued: HashSet::new(),
        confirm: None,
        local_message: None,
    })
}

pub fn hide_srpg_shop_overlay(state: &mut SrpgShopOverlayState) {
    *state = SrpgShopOverlayState::Hidden;
}

pub fn update_srpg_shop_overlay(state: &mut SrpgShopOverlayState, snapshot: &SrpgShopSnapshot) {
    let SrpgShopOverlayState::Visible(overlay) = state else {
        return;
    };
    for (index, shop_id) in SRPG_SHOP_IDS.into_iter().enumerate() {
        let len = snapshot
            .shops
            .iter()
            .find(|shop| shop.id == shop_id)
            .map_or(0, |shop| shop.items.len());
        overlay.item_indices[index] = overlay.item_indices[index].min(len);
    }
    if snapshot.phase == SrpgShopPhase::Ready
        && overlay
            .local_message
            .as_deref()
            .is_some_and(|message| message.starts_with("Purchasing "))
    {
        overlay.local_message = snapshot.message.clone();
    }
    if snapshot.phase != SrpgShopPhase::Ready {
        overlay.confirm = None;
    }
}

pub fn move_srpg_shop_selection(
    state: &mut SrpgShopOverlayState,
    snapshot: &SrpgShopSnapshot,
    delta: isize,
) -> bool {
    if snapshot.phase != SrpgShopPhase::Ready {
        return false;
    }
    let SrpgShopOverlayState::Visible(overlay) = state else {
        return false;
    };
    move_item(overlay, snapshot, delta) == SrpgShopInputOutcome::ChangedSelection
}

pub fn page_srpg_shop_selection(
    state: &mut SrpgShopOverlayState,
    snapshot: &SrpgShopSnapshot,
    direction: isize,
) -> bool {
    if snapshot.phase != SrpgShopPhase::Ready {
        return false;
    }
    let SrpgShopOverlayState::Visible(overlay) = state else {
        return false;
    };
    let Some(shop) = active_shop(overlay, snapshot) else {
        return false;
    };
    let selected = &mut overlay.item_indices[overlay.shop_index];
    let next = if direction < 0 {
        selected.saturating_sub(VIEW_ROWS)
    } else {
        selected.saturating_add(VIEW_ROWS).min(shop.items.len())
    };
    if next == *selected {
        return false;
    }
    *selected = next;
    overlay.local_message = None;
    true
}

pub fn handle_srpg_shop_input(
    state: &mut SrpgShopOverlayState,
    event: &InputEvent,
    snapshot: &SrpgShopSnapshot,
) -> SrpgShopInputOutcome {
    if !event.pressed {
        return SrpgShopInputOutcome::None;
    }
    let SrpgShopOverlayState::Visible(overlay) = state else {
        return SrpgShopInputOutcome::None;
    };

    if overlay.confirm.is_some() {
        return handle_confirm_input(overlay, event.action);
    }
    if matches!(
        snapshot.phase,
        SrpgShopPhase::Loading | SrpgShopPhase::Purchasing
    ) {
        return close_input(state, event.action);
    }

    match event.action {
        VirtualAction::p1_left
        | VirtualAction::p1_menu_left
        | VirtualAction::p2_left
        | VirtualAction::p2_menu_left => {
            overlay.shop_index = (overlay.shop_index + SHOPS.len() - 1) % SHOPS.len();
            overlay.local_message = None;
            SrpgShopInputOutcome::ChangedSelection
        }
        VirtualAction::p1_right
        | VirtualAction::p1_menu_right
        | VirtualAction::p2_right
        | VirtualAction::p2_menu_right => {
            overlay.shop_index = (overlay.shop_index + 1) % SHOPS.len();
            overlay.local_message = None;
            SrpgShopInputOutcome::ChangedSelection
        }
        VirtualAction::p1_up
        | VirtualAction::p1_menu_up
        | VirtualAction::p2_up
        | VirtualAction::p2_menu_up => move_item(overlay, snapshot, -1),
        VirtualAction::p1_down
        | VirtualAction::p1_menu_down
        | VirtualAction::p2_down
        | VirtualAction::p2_menu_down => move_item(overlay, snapshot, 1),
        VirtualAction::p1_start | VirtualAction::p2_start => activate_item(overlay, snapshot),
        VirtualAction::p1_back
        | VirtualAction::p2_back
        | VirtualAction::p1_select
        | VirtualAction::p2_select => {
            hide_srpg_shop_overlay(state);
            SrpgShopInputOutcome::Closed
        }
        _ => SrpgShopInputOutcome::None,
    }
}

fn close_input(state: &mut SrpgShopOverlayState, action: VirtualAction) -> SrpgShopInputOutcome {
    if matches!(
        action,
        VirtualAction::p1_back
            | VirtualAction::p2_back
            | VirtualAction::p1_select
            | VirtualAction::p2_select
    ) {
        hide_srpg_shop_overlay(state);
        SrpgShopInputOutcome::Closed
    } else {
        SrpgShopInputOutcome::None
    }
}

fn handle_confirm_input(
    overlay: &mut SrpgShopOverlayStateData,
    action: VirtualAction,
) -> SrpgShopInputOutcome {
    match action {
        VirtualAction::p1_start | VirtualAction::p2_start => {
            let confirm = overlay
                .confirm
                .take()
                .expect("purchase confirmation is visible");
            overlay.local_message = Some(format!("Purchasing {}...", confirm.name));
            SrpgShopInputOutcome::Purchase {
                shop_id: confirm.shop_id,
                item_id: confirm.item_id,
                type_id: confirm.type_id,
            }
        }
        VirtualAction::p1_back
        | VirtualAction::p2_back
        | VirtualAction::p1_select
        | VirtualAction::p2_select => {
            overlay.confirm = None;
            overlay.local_message = Some("Purchase canceled.".to_string());
            SrpgShopInputOutcome::ChangedSelection
        }
        _ => SrpgShopInputOutcome::None,
    }
}

fn move_item(
    overlay: &mut SrpgShopOverlayStateData,
    snapshot: &SrpgShopSnapshot,
    delta: isize,
) -> SrpgShopInputOutcome {
    let Some(shop) = active_shop(overlay, snapshot) else {
        return SrpgShopInputOutcome::None;
    };
    let len = shop.items.len() + 1;
    if len <= 1 {
        return SrpgShopInputOutcome::None;
    }
    let selected = &mut overlay.item_indices[overlay.shop_index];
    *selected = ((*selected as isize + delta).rem_euclid(len as isize)) as usize;
    overlay.local_message = None;
    SrpgShopInputOutcome::ChangedSelection
}

fn activate_item(
    overlay: &mut SrpgShopOverlayStateData,
    snapshot: &SrpgShopSnapshot,
) -> SrpgShopInputOutcome {
    if snapshot.phase == SrpgShopPhase::Error {
        overlay.local_message = None;
        return SrpgShopInputOutcome::Refresh(overlay.side);
    }
    let Some(shop) = active_shop(overlay, snapshot) else {
        return SrpgShopInputOutcome::Refresh(overlay.side);
    };
    let selected = overlay.item_indices[overlay.shop_index];
    if selected == 0 {
        return download_all(overlay, shop);
    }
    let Some(item) = shop.items.get(selected - 1) else {
        return SrpgShopInputOutcome::None;
    };
    if item.downloaded {
        overlay.local_message = Some("This song is already downloaded here.".to_string());
        return SrpgShopInputOutcome::ChangedSelection;
    }
    if let Some(url) = item.download_url.as_ref() {
        if !overlay.queued.insert(queue_key(shop.id, item)) {
            overlay.local_message = Some("This unlock is already queued.".to_string());
            return SrpgShopInputOutcome::ChangedSelection;
        }
        overlay.local_message = Some(format!("Queued {} for download.", item.name));
        return SrpgShopInputOutcome::Download {
            shop_id: shop.id,
            name: item.name.clone(),
            url: url.clone(),
        };
    }
    let Some(cost) = item.cost else {
        overlay.local_message = Some("This item is not currently available.".to_string());
        return SrpgShopInputOutcome::ChangedSelection;
    };
    if cost > shop.balance {
        overlay.local_message = Some(format!(
            "Need {} more {}.",
            format_number(cost - shop.balance),
            SHOPS[overlay.shop_index].currency
        ));
        return SrpgShopInputOutcome::ChangedSelection;
    }
    overlay.confirm = Some(PurchaseConfirm {
        shop_id: shop.id,
        item_id: item.item_id.clone(),
        type_id: item.type_id,
        name: item.name.clone(),
        cost,
    });
    SrpgShopInputOutcome::ChangedSelection
}

fn download_all(overlay: &mut SrpgShopOverlayStateData, shop: &SrpgShop) -> SrpgShopInputOutcome {
    let downloads = shop
        .items
        .iter()
        .filter(|item| !item.downloaded)
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

fn queue_key(shop_id: u32, item: &SrpgShopItem) -> String {
    format!("{shop_id}:{}", item.item_id)
}

fn active_shop<'a>(
    overlay: &SrpgShopOverlayStateData,
    snapshot: &'a SrpgShopSnapshot,
) -> Option<&'a SrpgShop> {
    let shop_id = SRPG_SHOP_IDS[overlay.shop_index];
    snapshot.shops.iter().find(|shop| shop.id == shop_id)
}

pub fn build_srpg_shop_overlay(
    state: &SrpgShopOverlayState,
    snapshot: &SrpgShopSnapshot,
    machine_font: MachineFont,
) -> Option<Vec<Actor>> {
    let SrpgShopOverlayState::Visible(overlay) = state else {
        return None;
    };
    let mut actors = Vec::with_capacity(48);
    let cx = screen_center_x();
    let cy = screen_center_y();
    let meta = SHOPS[overlay.shop_index];
    let header_font = machine_font_key(machine_font, FontRole::Header);
    let bold_font = machine_font_key(machine_font, FontRole::Bold);

    actors.push(act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0): zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, 0.88): z(Z)
    ));
    actors.push(act!(quad:
        align(0.5, 0.5): xy(cx, cy): zoomto(PANEL_W + 4.0, PANEL_H + 4.0):
        diffuse(meta.tint[0], meta.tint[1], meta.tint[2], 1.0): z(Z + 1)
    ));
    actors.push(act!(sprite(meta.image):
        align(0.5, 0.5): xy(cx, cy): zoomto(PANEL_W, SHOP_IMAGE_H):
        diffuse(0.72, 0.72, 0.72, 1.0): z(Z + 2)
    ));
    actors.push(act!(quad:
        align(0.5, 0.5): xy(cx, cy): zoomto(PANEL_W, PANEL_H):
        diffuse(0.0, 0.0, 0.0, 0.38): z(Z + 3)
    ));
    actors.push(act!(quad:
        align(0.5, 0.5): xy(cx, cy + HEADER_Y): zoomto(PANEL_W, HEADER_H):
        diffuse(0.0, 0.0, 0.0, 0.86): z(Z + 4)
    ));
    actors.push(act!(text:
        font(header_font): settext("SRPG SHOP"):
        align(0.5, 0.5): xy(cx - 143.0, cy + HEADER_CONTENT_Y): maxwidth(250.0):
        zoom(0.42): diffuse(1.0, 1.0, 1.0, 1.0): z(Z + 5): horizalign(center)
    ));

    push_tabs(&mut actors, overlay.shop_index, cx, cy, bold_font);
    push_shop_heading(
        &mut actors,
        active_shop(overlay, snapshot),
        meta,
        cx,
        cy,
        bold_font,
    );
    match snapshot.phase {
        SrpgShopPhase::Idle | SrpgShopPhase::Loading => push_status(
            &mut actors,
            snapshot
                .message
                .as_deref()
                .unwrap_or("Loading SRPG10 shops..."),
            [1.0, 1.0, 1.0, 1.0],
            cx,
            cy,
            bold_font,
        ),
        SrpgShopPhase::Error => push_status(
            &mut actors,
            snapshot
                .message
                .as_deref()
                .unwrap_or("Unable to load SRPG10 shops."),
            [1.0, 0.45, 0.35, 1.0],
            cx,
            cy,
            bold_font,
        ),
        SrpgShopPhase::Ready | SrpgShopPhase::Purchasing => {
            push_catalog(&mut actors, overlay, snapshot, meta, cx, cy, bold_font);
        }
    }
    push_footer(&mut actors, snapshot.phase, cx, cy);
    if let Some(confirm) = overlay.confirm.as_ref() {
        push_confirmation(&mut actors, confirm, meta, cx, cy, header_font, bold_font);
    }
    Some(actors)
}

fn push_tabs(actors: &mut Vec<Actor>, selected: usize, cx: f32, cy: f32, bold_font: &'static str) {
    let start_x = cx + 53.0;
    for (index, shop) in SHOPS.into_iter().enumerate() {
        let x = start_x + index as f32 * 64.0;
        let active = index == selected;
        actors.push(act!(quad:
            align(0.5, 0.5): xy(x, cy + HEADER_CONTENT_Y): zoomto(58.0, 30.0):
            diffuse(shop.tint[0], shop.tint[1], shop.tint[2], if active { 0.92 } else { 0.30 }):
            z(Z + 5)
        ));
        actors.push(act!(text:
            font(bold_font): settext(shop.short_name):
            align(0.5, 0.5): xy(x, cy + HEADER_CONTENT_Y): zoom(0.25):
            diffuse(1.0, 1.0, 1.0, if active { 1.0 } else { 0.65 }): z(Z + 6):
            horizalign(center)
        ));
    }
}

fn push_shop_heading(
    actors: &mut Vec<Actor>,
    shop: Option<&SrpgShop>,
    meta: ShopMeta,
    cx: f32,
    cy: f32,
    bold_font: &'static str,
) {
    actors.push(act!(text:
        font(bold_font): settext(meta.name):
        align(0.0, 0.5): xy(cx - PANEL_W * 0.5 + 15.0, cy - 153.0): zoom(0.38):
        maxwidth(290.0): diffuse(meta.tint[0], meta.tint[1], meta.tint[2], 1.0):
        z(Z + 6): horizalign(left)
    ));
    if let Some(shop) = shop {
        actors.push(act!(text:
            font("miso"): settext(format!("{} {}", format_number(shop.balance), meta.currency)):
            align(0.0, 0.5): xy(cx - PANEL_W * 0.5 + 15.0, cy - 132.0): zoom(0.83):
            diffuse(1.0, 1.0, 1.0, 1.0): z(Z + 6): horizalign(left)
        ));
    }
}

fn push_catalog(
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
    let list_h = (visible_rows - 1) as f32 * ROW_H + (ROW_H - 3.0) + LIST_MARGIN * 2.0;
    let list_y = LIST_Y + (visible_rows - 1) as f32 * ROW_H * 0.5;
    actors.push(act!(quad:
        align(0.5, 0.5): xy(cx + LIST_X, cy + list_y): zoomto(LIST_W, list_h):
        diffuse(0.0, 0.0, 0.0, 0.78): z(Z + 4)
    ));
    for (slot, row_index) in (start..row_count.min(start + VIEW_ROWS)).enumerate() {
        let y = cy + LIST_Y + slot as f32 * ROW_H;
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
            align(0.0, 0.5): xy(cx + LIST_X - LIST_W * 0.5 + 9.0, y - 5.0): zoom(0.25):
            maxwidth(205.0): diffuse(1.0, 1.0, 1.0, if active { 1.0 } else { 0.76 }):
            z(Z + 6): horizalign(left)
        ));
        actors.push(act!(text:
            font("miso"): settext(detail):
            align(0.0, 0.5): xy(cx + LIST_X - LIST_W * 0.5 + 9.0, y + 8.0): zoom(0.67):
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

fn push_bulk_detail(
    actors: &mut Vec<Actor>,
    overlay: &SrpgShopOverlayStateData,
    shop: &SrpgShop,
    meta: ShopMeta,
    cx: f32,
    cy: f32,
    bold_font: &'static str,
) {
    let x = cx - PANEL_W * 0.5 + 15.0;
    let ready = ready_count(overlay, shop);
    let downloaded = shop.items.iter().filter(|item| item.downloaded).count();
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

fn push_item_detail(
    actors: &mut Vec<Actor>,
    overlay: &SrpgShopOverlayStateData,
    shop: &SrpgShop,
    item: &SrpgShopItem,
    meta: ShopMeta,
    cx: f32,
    cy: f32,
    bold_font: &'static str,
) {
    let x = cx - PANEL_W * 0.5 + 15.0;
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

fn push_status(
    actors: &mut Vec<Actor>,
    message: &str,
    color: [f32; 4],
    cx: f32,
    cy: f32,
    bold_font: &'static str,
) {
    actors.push(act!(quad:
        align(0.5, 0.5): xy(cx, cy + 20.0): zoomto(520.0, 120.0):
        diffuse(0.0, 0.0, 0.0, 0.82): z(Z + 5)
    ));
    actors.push(act!(text:
        font(bold_font): settext(message.to_owned()):
        align(0.5, 0.5): xy(cx, cy + 10.0): zoom(0.34): wrapwidthpixels(1000.0):
        maxwidth(500.0): diffuse(color[0], color[1], color[2], color[3]): z(Z + 6):
        horizalign(center)
    ));
}

fn push_footer(actors: &mut Vec<Actor>, phase: SrpgShopPhase, cx: f32, cy: f32) {
    let hint = match phase {
        SrpgShopPhase::Error => "&START; retry    &SELECT; / BACK close",
        SrpgShopPhase::Loading | SrpgShopPhase::Purchasing => {
            "Please wait    &SELECT; / BACK close"
        }
        _ => {
            "&MENULEFT; &MENURIGHT; shop    &MENUUP; &MENUDOWN; item    &START; action    &SELECT; close"
        }
    };
    actors.push(act!(quad:
        align(0.5, 0.5): xy(cx, cy + PANEL_H * 0.5 - 18.0): zoomto(PANEL_W, 36.0):
        diffuse(0.0, 0.0, 0.0, 0.90): z(Z + 5)
    ));
    actors.push(act!(text:
        font("miso"): settext(hint): align(0.5, 0.5):
        xy(cx, cy + PANEL_H * 0.5 - 18.0): zoom(0.72): maxwidth(PANEL_W - 20.0):
        diffuse(1.0, 1.0, 1.0, 0.85): z(Z + 6): horizalign(center)
    ));
}

fn push_confirmation(
    actors: &mut Vec<Actor>,
    confirm: &PurchaseConfirm,
    meta: ShopMeta,
    cx: f32,
    cy: f32,
    header_font: &'static str,
    bold_font: &'static str,
) {
    actors.push(act!(quad:
        align(0.5, 0.5): xy(cx, cy): zoomto(430.0, 170.0):
        diffuse(meta.tint[0], meta.tint[1], meta.tint[2], 1.0): z(Z + 20)
    ));
    actors.push(act!(quad:
        align(0.5, 0.5): xy(cx, cy): zoomto(426.0, 166.0):
        diffuse(0.02, 0.02, 0.02, 0.98): z(Z + 21)
    ));
    actors.push(act!(text:
        font(header_font): settext("CONFIRM PURCHASE"):
        align(0.5, 0.5): xy(cx, cy - 55.0): zoom(0.37):
        diffuse(meta.tint[0], meta.tint[1], meta.tint[2], 1.0): z(Z + 22): horizalign(center)
    ));
    actors.push(act!(text:
        font(bold_font):
        settext(format!("{}\n{} {}", confirm.name, format_number(confirm.cost), meta.currency)):
        align(0.5, 0.5): xy(cx, cy - 6.0): zoom(0.30): maxwidth(390.0):
        diffuse(1.0, 1.0, 1.0, 1.0): z(Z + 22): horizalign(center)
    ));
    actors.push(act!(text:
        font("miso"): settext("Press &START; to buy one.  Press &SELECT; / BACK to cancel."):
        align(0.5, 0.5): xy(cx, cy + 58.0): zoom(0.70): maxwidth(400.0):
        diffuse(1.0, 1.0, 1.0, 0.9): z(Z + 22): horizalign(center)
    ));
}

fn bulk_row_detail(overlay: &SrpgShopOverlayStateData, shop: &SrpgShop) -> String {
    let ready = ready_count(overlay, shop);
    let downloaded = shop.items.iter().filter(|item| item.downloaded).count();
    format!("{ready} READY  •  {downloaded} DOWNLOADED")
}

fn ready_count(overlay: &SrpgShopOverlayStateData, shop: &SrpgShop) -> usize {
    shop.items
        .iter()
        .filter(|item| {
            item.download_url.is_some()
                && !item.downloaded
                && !overlay.queued.contains(&queue_key(shop.id, item))
        })
        .count()
}

fn item_row_detail(item: &SrpgShopItem, currency: &str, queued: bool) -> String {
    if item.downloaded {
        return "DOWNLOADED".to_string();
    }
    if queued {
        return "DOWNLOAD QUEUED".to_string();
    }
    if item.owned {
        let new = if item.site_downloaded {
            ""
        } else {
            "  •  NEW"
        };
        return format!("OWNED  •  READY TO DOWNLOAD{new}");
    }
    let kind = match item.kind {
        SrpgShopItemKind::Song => match (item.difficulty, item.bpm) {
            (Some(level), Some(bpm)) => format!("LV {level}  •  {bpm} BPM"),
            _ => "SONG UNLOCK".to_string(),
        },
        SrpgShopItemKind::Relic => "RELIC".to_string(),
    };
    item.cost.map_or(kind.clone(), |cost| {
        format!("{kind}  •  {} {currency}", format_number(cost))
    })
}

fn active_message(
    item: &SrpgShopItem,
    balance: u64,
    currency: &str,
    queued: bool,
) -> Option<String> {
    if item.downloaded {
        return Some("This song is already downloaded here.".to_string());
    }
    if queued {
        return Some("This song is queued for download.".to_string());
    }
    if item.download_url.is_some() {
        return Some("Press START to download this owned unlock.".to_string());
    }
    item.cost.map(|cost| {
        if cost <= balance {
            format!("Press START to buy for {} {currency}.", format_number(cost))
        } else {
            format!("Insufficient {currency}.")
        }
    })
}

fn format_number(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, ch) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_core::input::InputSource;
    use std::time::Instant;

    fn input(action: VirtualAction) -> InputEvent {
        let now = Instant::now();
        InputEvent {
            action,
            input_slot: 0,
            pressed: true,
            source: InputSource::Gamepad,
            timestamp: now,
            timestamp_host_nanos: 0,
            stored_at: now,
            emitted_at: now,
        }
    }

    fn snapshot(owned: bool, downloaded: bool) -> SrpgShopSnapshot {
        SrpgShopSnapshot {
            phase: SrpgShopPhase::Ready,
            shops: vec![SrpgShop {
                id: 3,
                balance: 2_000,
                items: vec![SrpgShopItem {
                    item_id: "7".to_string(),
                    kind: SrpgShopItemKind::Song,
                    name: "Fast Song".to_string(),
                    description: String::new(),
                    effect: String::new(),
                    cost: Some(1_234),
                    difficulty: Some(14),
                    bpm: Some(180),
                    type_id: 1,
                    owned,
                    site_downloaded: false,
                    downloaded,
                    download_url: owned.then(|| "https://example.test/song.zip".to_string()),
                }],
            }],
            message: None,
        }
    }

    #[test]
    fn purchase_requires_two_start_presses() {
        let mut state = show_srpg_shop_overlay(PlayerSide::P1);
        assert_eq!(
            handle_srpg_shop_input(
                &mut state,
                &input(VirtualAction::p1_down),
                &snapshot(false, false)
            ),
            SrpgShopInputOutcome::ChangedSelection
        );
        assert_eq!(
            handle_srpg_shop_input(
                &mut state,
                &input(VirtualAction::p1_start),
                &snapshot(false, false)
            ),
            SrpgShopInputOutcome::ChangedSelection
        );
        assert_eq!(
            handle_srpg_shop_input(
                &mut state,
                &input(VirtualAction::p1_start),
                &snapshot(false, false)
            ),
            SrpgShopInputOutcome::Purchase {
                shop_id: 3,
                item_id: "7".to_string(),
                type_id: 1,
            }
        );
    }

    #[test]
    fn owned_song_queues_download_immediately() {
        let mut state = show_srpg_shop_overlay(PlayerSide::P1);
        handle_srpg_shop_input(
            &mut state,
            &input(VirtualAction::p1_down),
            &snapshot(true, false),
        );
        assert_eq!(
            handle_srpg_shop_input(
                &mut state,
                &input(VirtualAction::p1_start),
                &snapshot(true, false)
            ),
            SrpgShopInputOutcome::Download {
                shop_id: 3,
                name: "Fast Song".to_string(),
                url: "https://example.test/song.zip".to_string(),
            }
        );
    }

    #[test]
    fn download_all_queues_every_ready_song() {
        let mut state = show_srpg_shop_overlay(PlayerSide::P1);
        assert_eq!(
            handle_srpg_shop_input(
                &mut state,
                &input(VirtualAction::p1_start),
                &snapshot(true, false)
            ),
            SrpgShopInputOutcome::DownloadAll {
                shop_id: 3,
                downloads: vec![SrpgShopDownload {
                    name: "Fast Song".to_string(),
                    url: "https://example.test/song.zip".to_string(),
                }],
            }
        );
    }

    #[test]
    fn downloaded_song_is_not_queued_again() {
        let mut state = show_srpg_shop_overlay(PlayerSide::P1);
        assert_eq!(
            handle_srpg_shop_input(
                &mut state,
                &input(VirtualAction::p1_start),
                &snapshot(true, true)
            ),
            SrpgShopInputOutcome::ChangedSelection
        );
    }

    #[test]
    fn formats_shop_balances_with_grouping() {
        assert_eq!(format_number(1_234_567), "1,234,567");
    }

    #[test]
    fn faction_tabs_use_expected_order_and_colors() {
        assert_eq!(SRPG_SHOP_IDS, [3, 0, 2, 4]);
        assert_eq!(
            SHOPS.map(|shop| shop.short_name),
            ["SN", "DPRT", "FE", "NEP"]
        );
        assert_eq!(SHOPS[0].tint, [1.0, 0.40, 0.18]);
        assert_eq!(SHOPS[1].tint, [0.30, 0.84, 1.0]);
        assert_eq!(SHOPS[2].tint, [0.75, 0.46, 1.0]);
        assert_eq!(SHOPS[3].tint, [1.0, 0.72, 0.20]);
        assert!(((PANEL_W / SHOP_IMAGE_H) - 16.0 / 9.0).abs() < 0.0001);
        assert!(HEADER_CONTENT_Y < HEADER_Y);
        assert!(HEADER_CONTENT_Y - 15.0 >= -PANEL_H * 0.5);
    }

    #[test]
    fn repeated_navigation_uses_the_same_selection_move() {
        let mut state = show_srpg_shop_overlay(PlayerSide::P1);
        assert!(move_srpg_shop_selection(
            &mut state,
            &snapshot(false, false),
            1
        ));
        let SrpgShopOverlayState::Visible(overlay) = state else {
            panic!("shop should remain visible");
        };
        assert_eq!(overlay.item_indices[0], 1);
    }

    #[test]
    fn page_navigation_clamps_at_the_first_and_last_rows() {
        let mut state = show_srpg_shop_overlay(PlayerSide::P1);
        let snapshot = snapshot(false, false);

        assert!(page_srpg_shop_selection(&mut state, &snapshot, 1));
        assert!(!page_srpg_shop_selection(&mut state, &snapshot, 1));
        assert!(page_srpg_shop_selection(&mut state, &snapshot, -1));
        assert!(!page_srpg_shop_selection(&mut state, &snapshot, -1));
    }
}
