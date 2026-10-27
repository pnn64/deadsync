use crate::act;
use crate::fonts::machine_font_key;
use crate::screens::components::select_music::push_retained_overlay;
use deadlib_present::actors::Actor;
use deadlib_present::space::{screen_center_x, screen_center_y, screen_height, screen_width};
use deadsync_config::theme::MachineFont;
use deadsync_input::{InputEvent, VirtualAction};
use deadsync_online::srpg_shop::{
    SRPG_SHOP_IDS, SrpgShop, SrpgShopItem, SrpgShopItemKind, SrpgShopPhase, SrpgShopSnapshot,
};
use deadsync_profile::PlayerSide;
use deadsync_theme::FontRole;
use std::cell::RefCell;
use std::collections::HashSet;
use std::sync::Arc;

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
    queued: [HashSet<Box<str>>; SHOPS.len()],
    confirm: Option<PurchaseConfirm>,
    local_message: Option<String>,
    purchase_pending: bool,
    presentation_revision: u64,
    presentation: RefCell<Option<SrpgShopPresentation>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SrpgShopPresentationKey {
    revision: u64,
    machine_font: MachineFont,
    screen_width_bits: u32,
    screen_height_bits: u32,
}

#[derive(Clone, Debug)]
struct SrpgShopPresentation {
    key: SrpgShopPresentationKey,
    snapshot: Arc<SrpgShopSnapshot>,
    children: Arc<[Actor]>,
}

impl SrpgShopOverlayStateData {
    fn invalidate_presentation(&mut self) {
        self.presentation_revision = self.presentation_revision.wrapping_add(1);
        self.presentation.get_mut().take();
    }
}

#[derive(Clone, Debug)]
pub enum SrpgShopOverlayState {
    Hidden,
    Visible(Box<SrpgShopOverlayStateData>),
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

#[must_use]
pub fn show_srpg_shop_overlay(side: PlayerSide) -> SrpgShopOverlayState {
    SrpgShopOverlayState::Visible(Box::new(SrpgShopOverlayStateData {
        side,
        shop_index: 0,
        item_indices: [0; 4],
        queued: std::array::from_fn(|_| HashSet::new()),
        confirm: None,
        local_message: None,
        purchase_pending: false,
        presentation_revision: 0,
        presentation: RefCell::new(None),
    }))
}

pub fn hide_srpg_shop_overlay(state: &mut SrpgShopOverlayState) {
    *state = SrpgShopOverlayState::Hidden;
}

pub fn update_srpg_shop_overlay(state: &mut SrpgShopOverlayState, snapshot: &SrpgShopSnapshot) {
    let SrpgShopOverlayState::Visible(overlay) = state else {
        return;
    };
    let mut presentation_changed = false;
    for (index, shop_id) in SRPG_SHOP_IDS.into_iter().enumerate() {
        let len = snapshot
            .shops
            .iter()
            .find(|shop| shop.id == shop_id)
            .map_or(0, |shop| shop.items.len());
        let clamped = overlay.item_indices[index].min(len);
        presentation_changed |= clamped != overlay.item_indices[index];
        overlay.item_indices[index] = clamped;
    }
    if matches!(snapshot.phase, SrpgShopPhase::Ready | SrpgShopPhase::Error) && overlay.purchase_pending
    {
        presentation_changed |= overlay.local_message != snapshot.message;
        overlay.local_message.clone_from(&snapshot.message);
        overlay.purchase_pending = false;
    }
    if snapshot.phase != SrpgShopPhase::Ready {
        presentation_changed |= overlay.confirm.take().is_some();
    }
    if presentation_changed {
        overlay.invalidate_presentation();
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
    let changed = move_item(overlay, snapshot, delta) == SrpgShopInputOutcome::ChangedSelection;
    if changed {
        overlay.invalidate_presentation();
    }
    changed
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
    overlay.invalidate_presentation();
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
    overlay.invalidate_presentation();

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
            overlay.purchase_pending = true;
            overlay.local_message = Some(crate::i18n::tr_fmt("SrpgShop", "MessagePurchasing", &[("arg0", &(confirm.name).to_string())]).to_string());
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
            overlay.local_message = Some(crate::i18n::tr("SrpgShop", "MessagePurchaseCanceled").to_string());
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
    if !item.owned {
        let Some(cost) = item.cost else {
            overlay.local_message = Some(crate::i18n::tr("SrpgShop", "MessageThisItemIsNotCurrentlyAvailable").to_string());
            return SrpgShopInputOutcome::ChangedSelection;
        };
        if cost > shop.balance {
            overlay.local_message = Some(crate::i18n::tr_fmt("SrpgShop", "MessageNeedMore", &[("arg0", &(format_number(cost - shop.balance)).to_string()), ("arg1", &(SHOPS[overlay.shop_index].currency).to_string())]).to_string());
            return SrpgShopInputOutcome::ChangedSelection;
        }
        overlay.confirm = Some(PurchaseConfirm {
            shop_id: shop.id,
            item_id: item.item_id.clone(),
            type_id: item.type_id,
            name: item.name.clone(),
            cost,
        });
        return SrpgShopInputOutcome::ChangedSelection;
    }
    if item.downloaded {
        overlay.local_message = Some(crate::i18n::tr("SrpgShop", "MessageThisOwnedSongIsAlreadyDownloadedHere").to_string());
        return SrpgShopInputOutcome::ChangedSelection;
    }
    let Some(url) = item.download_url.as_ref() else {
        overlay.local_message = Some(crate::i18n::tr("SrpgShop", "MessageThisItemIsNotCurrentlyAvailable").to_string());
        return SrpgShopInputOutcome::ChangedSelection;
    };
    if !overlay.queued[overlay.shop_index].insert(item.item_id.as_str().into()) {
        overlay.local_message = Some(crate::i18n::tr("SrpgShop", "MessageThisUnlockIsAlreadyQueued").to_string());
        return SrpgShopInputOutcome::ChangedSelection;
    }
    overlay.local_message = Some(crate::i18n::tr_fmt("SrpgShop", "MessageQueuedForDownload", &[("arg0", &(item.name).to_string())]).to_string());
    SrpgShopInputOutcome::Download {
        shop_id: shop.id,
        name: item.name.clone(),
        url: url.clone(),
    }
}

fn download_all(overlay: &mut SrpgShopOverlayStateData, shop: &SrpgShop) -> SrpgShopInputOutcome {
    let downloads = shop
        .items
        .iter()
        .filter(|item| item.owned && !item.downloaded)
        .filter_map(|item| {
            let url = item.download_url.as_ref()?;
            overlay.queued[overlay.shop_index]
                .insert(item.item_id.as_str().into())
                .then(|| SrpgShopDownload {
                    name: item.name.clone(),
                    url: url.clone(),
                })
        })
        .collect::<Vec<_>>();
    if downloads.is_empty() {
        overlay.local_message = Some(crate::i18n::tr("SrpgShop", "MessageAllOwnedSongsAreDownloadedOrQueued").to_string());
        return SrpgShopInputOutcome::ChangedSelection;
    }
    overlay.local_message = Some(crate::i18n::tr_fmt("SrpgShop", "MessageQueuedSongsForDownload", &[("arg0", &(downloads.len()).to_string())]).to_string());
    SrpgShopInputOutcome::DownloadAll {
        shop_id: shop.id,
        downloads,
    }
}

fn active_shop<'a>(
    overlay: &SrpgShopOverlayStateData,
    snapshot: &'a SrpgShopSnapshot,
) -> Option<&'a SrpgShop> {
    let shop_id = SRPG_SHOP_IDS[overlay.shop_index];
    snapshot.shops.iter().find(|shop| shop.id == shop_id)
}

pub fn push_srpg_shop_overlay(
    actors: &mut Vec<Actor>,
    state: &SrpgShopOverlayState,
    snapshot: &Arc<SrpgShopSnapshot>,
    machine_font: MachineFont,
) -> bool {
    let SrpgShopOverlayState::Visible(overlay) = state else {
        return false;
    };
    let key = SrpgShopPresentationKey {
        revision: overlay.presentation_revision,
        machine_font,
        screen_width_bits: screen_width().to_bits(),
        screen_height_bits: screen_height().to_bits(),
    };
    let cached = overlay
        .presentation
        .borrow()
        .as_ref()
        .filter(|presentation| {
            presentation.key == key && Arc::ptr_eq(&presentation.snapshot, snapshot)
        })
        .map(|presentation| Arc::clone(&presentation.children));
    let children = cached.unwrap_or_else(|| {
        let mut children = Vec::with_capacity(48);
        push_srpg_shop_overlay_unreserved(&mut children, overlay, snapshot, machine_font);
        let children = Arc::<[Actor]>::from(children);
        *overlay.presentation.borrow_mut() = Some(SrpgShopPresentation {
            key,
            snapshot: Arc::clone(snapshot),
            children: Arc::clone(&children),
        });
        children
    });
    push_retained_overlay(actors, children);
    true
}

fn push_srpg_shop_overlay_unreserved(
    actors: &mut Vec<Actor>,
    overlay: &SrpgShopOverlayStateData,
    snapshot: &SrpgShopSnapshot,
    machine_font: MachineFont,
) {
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
        font(header_font): settext(crate::i18n::tr("SrpgShop", "Title")):
        align(0.5, 0.5): xy(cx - 143.0, cy + HEADER_CONTENT_Y): maxwidth(250.0):
        zoom(0.42): diffuse(1.0, 1.0, 1.0, 1.0): z(Z + 5): horizalign(center)
    ));

    push_tabs(actors, overlay.shop_index, cx, cy, bold_font);
    push_shop_heading(
        actors,
        active_shop(overlay, snapshot),
        meta,
        cx,
        cy,
        bold_font,
    );
    match snapshot.phase {
        SrpgShopPhase::Idle | SrpgShopPhase::Loading => push_status(
            actors,
            snapshot
                .message
                .as_deref()
                .unwrap_or(&crate::i18n::tr("SrpgShop", "MessageLoadingSRPG10Shops")),
            [1.0, 1.0, 1.0, 1.0],
            cx,
            cy,
            bold_font,
        ),
        SrpgShopPhase::Error => push_status(
            actors,
            snapshot
                .message
                .as_deref()
                .unwrap_or(&crate::i18n::tr("SrpgShop", "MessageUnableToLoadSRPG10Shops")),
            [1.0, 0.45, 0.35, 1.0],
            cx,
            cy,
            bold_font,
        ),
        SrpgShopPhase::Ready | SrpgShopPhase::Purchasing => {
            push_catalog(actors, overlay, snapshot, meta, cx, cy, bold_font);
        }
    }
    push_footer(actors, snapshot.phase, cx, cy);
    if let Some(confirm) = overlay.confirm.as_ref() {
        push_confirmation(actors, confirm, meta, cx, cy, header_font, bold_font);
    }
}

fn push_tabs(actors: &mut Vec<Actor>, selected: usize, cx: f32, cy: f32, bold_font: &'static str) {
    let start_x = cx + 53.0;
    for (index, shop) in SHOPS.into_iter().enumerate() {
        let x = (index as f32).mul_add(64.0, start_x);
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
        align(0.0, 0.5): xy(PANEL_W.mul_add(-0.5, cx) + 15.0, cy - 153.0): zoom(0.38):
        maxwidth(290.0): diffuse(meta.tint[0], meta.tint[1], meta.tint[2], 1.0):
        z(Z + 6): horizalign(left)
    ));
    if let Some(shop) = shop {
        actors.push(act!(text:
            font("miso"): settext(format!("{} {}", format_number(shop.balance), meta.currency)):
            align(0.0, 0.5): xy(PANEL_W.mul_add(-0.5, cx) + 15.0, cy - 132.0): zoom(0.83):
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
            &crate::i18n::tr("SrpgShop", "MessageThisShopIsUnavailable"),
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
            &crate::i18n::tr("SrpgShop", "MessageNothingIsCurrentlyListedHere"),
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
                crate::i18n::tr("SrpgShop", "MessageDOWNLOADALLSONGS").to_string(),
                bulk_row_detail(overlay, shop),
            )
        } else {
            let item = &shop.items[row_index - 1];
            (
                item.name.clone(),
                item_row_detail(
                    item,
                    meta.currency,
                    overlay.queued[overlay.shop_index].contains(item.item_id.as_str()),
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

fn push_bulk_detail(
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
            crate::i18n::tr("SrpgShop", "MessageAllOwnedSongsAreDownloadedOrQueued").to_string()
        } else {
            crate::i18n::tr_fmt("SrpgShop", "MessagePressSTARTToDownloadAllReadyReadySongs", &[("ready", &(ready).to_string())]).to_string()
        }
    });
    actors.push(act!(quad:
        align(0.0, 0.0): xy(x - 5.0, cy - 115.0): zoomto(286.0, 250.0):
        diffuse(0.0, 0.0, 0.0, 0.72): z(Z + 4)
    ));
    actors.push(act!(text:
        font(bold_font): settext(crate::i18n::tr("SrpgShop", "DownloadAll")):
        align(0.0, 0.5): xy(x + 4.0, cy - 99.0): zoom(0.34): maxwidth(260.0):
        diffuse(1.0, 1.0, 1.0, 1.0): z(Z + 6): horizalign(left)
    ));
    actors.push(act!(text:
        font("miso"): settext(crate::i18n::tr_fmt("SrpgShop", "MessageReadyREADYDownloadedDOWNLOADED", &[("ready", &(ready).to_string()), ("downloaded", &(downloaded).to_string())]).to_string()):
        align(0.0, 0.0): xy(x + 4.0, cy - 78.0): zoom(0.72): maxwidth(260.0):
        diffuse(meta.tint[0], meta.tint[1], meta.tint[2], 1.0): z(Z + 6): horizalign(left)
    ));
    actors.push(act!(text:
        font("miso"): settext(crate::i18n::tr("SrpgShop", "DownloadAllHelp")):
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
            overlay.queued[overlay.shop_index].contains(item.item_id.as_str()),
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
        SrpgShopPhase::Error => crate::i18n::tr("SrpgShop", "MessageSTARTRetrySELECTBACKClose"),
        SrpgShopPhase::Loading | SrpgShopPhase::Purchasing => {
            crate::i18n::tr("SrpgShop", "MessagePleaseWaitSELECTBACKClose")
        }
        _ => {
            crate::i18n::tr("SrpgShop", "MessageMENULEFTMENURIGHTShopMENUUPMENUDOWNItemSTARTActionSELECTClose")
        }
    };
    actors.push(act!(quad:
        align(0.5, 0.5): xy(cx, PANEL_H.mul_add(0.5, cy) - 18.0): zoomto(PANEL_W, 36.0):
        diffuse(0.0, 0.0, 0.0, 0.90): z(Z + 5)
    ));
    actors.push(act!(text:
        font("miso"): settext(hint): align(0.5, 0.5):
        xy(cx, PANEL_H.mul_add(0.5, cy) - 18.0): zoom(0.72): maxwidth(PANEL_W - 20.0):
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
        font(header_font): settext(crate::i18n::tr("SrpgShop", "ConfirmPurchase")):
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
        font("miso"): settext(crate::i18n::tr("SrpgShop", "PurchaseHint")):
        align(0.5, 0.5): xy(cx, cy + 58.0): zoom(0.70): maxwidth(400.0):
        diffuse(1.0, 1.0, 1.0, 0.9): z(Z + 22): horizalign(center)
    ));
}

fn bulk_row_detail(overlay: &SrpgShopOverlayStateData, shop: &SrpgShop) -> String {
    let ready = ready_count(overlay, shop);
    let downloaded = shop
        .items
        .iter()
        .filter(|item| item.owned && item.downloaded)
        .count();
    crate::i18n::tr_fmt("SrpgShop", "MessageReadyREADYDownloadedDOWNLOADED", &[("ready", &(ready).to_string()), ("downloaded", &(downloaded).to_string())]).to_string()
}

fn ready_count(overlay: &SrpgShopOverlayStateData, shop: &SrpgShop) -> usize {
    shop.items
        .iter()
        .filter(|item| {
            item.owned
                && item.download_url.is_some()
                && !item.downloaded
                && !overlay.queued[overlay.shop_index].contains(item.item_id.as_str())
        })
        .count()
}

fn item_row_detail(item: &SrpgShopItem, currency: &str, queued: bool) -> String {
    if !item.owned {
        let kind = match item.kind {
            SrpgShopItemKind::Song => match (item.difficulty, item.bpm) {
                (Some(level), Some(bpm)) => format!("LV {level}  •  {bpm} BPM"),
                _ => crate::i18n::tr("SrpgShop", "MessageSONGUNLOCK").to_string(),
            },
            SrpgShopItemKind::Relic => crate::i18n::tr("SrpgShop", "MessageRELIC").to_string(),
        };
        return item.cost.map_or_else(
            || kind.clone(),
            |cost| format!("{kind}  •  {} {currency}", format_number(cost)),
        );
    }
    if item.downloaded {
        return crate::i18n::tr("SrpgShop", "MessageOWNEDDOWNLOADED").to_string();
    }
    if queued {
        return crate::i18n::tr("SrpgShop", "MessageDOWNLOADQUEUED").to_string();
    }
    let new = if item.site_downloaded { String::new() } else { crate::i18n::tr("SrpgShop", "MessageNEW").to_string() };
    crate::i18n::tr_fmt("SrpgShop", "MessageOWNEDREADYTODOWNLOADNew", &[("new", &(new).to_string())]).to_string()
}

fn active_message(
    item: &SrpgShopItem,
    balance: u64,
    currency: &str,
    queued: bool,
) -> Option<String> {
    if !item.owned {
        return item.cost.map(|cost| {
            if cost <= balance {
                crate::i18n::tr_fmt("SrpgShop", "MessagePressSTARTToBuyForCurrency", &[("arg0", &(format_number(cost)).to_string()), ("currency", &(currency).to_string())]).to_string()
            } else {
                crate::i18n::tr_fmt("SrpgShop", "MessageInsufficientCurrency", &[("currency", &(currency).to_string())]).to_string()
            }
        });
    }
    if item.downloaded {
        return Some(crate::i18n::tr("SrpgShop", "MessageThisOwnedSongIsAlreadyDownloadedHere").to_string());
    }
    if queued {
        return Some(crate::i18n::tr("SrpgShop", "MessageThisSongIsQueuedForDownload").to_string());
    }
    if item.download_url.is_some() {
        return Some(crate::i18n::tr("SrpgShop", "MessagePressSTARTToDownloadThisOwnedUnlock").to_string());
    }
    None
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
    fn downloaded_but_unowned_song_still_requires_purchase() {
        let mut state = show_srpg_shop_overlay(PlayerSide::P1);
        handle_srpg_shop_input(
            &mut state,
            &input(VirtualAction::p1_down),
            &snapshot(false, true),
        );
        assert_eq!(
            handle_srpg_shop_input(
                &mut state,
                &input(VirtualAction::p1_start),
                &snapshot(false, true)
            ),
            SrpgShopInputOutcome::ChangedSelection
        );
        assert_eq!(
            handle_srpg_shop_input(
                &mut state,
                &input(VirtualAction::p1_start),
                &snapshot(false, true)
            ),
            SrpgShopInputOutcome::Purchase {
                shop_id: 3,
                item_id: "7".to_string(),
                type_id: 1,
            }
        );
    }

    #[test]
    fn downloaded_but_unowned_song_keeps_the_normal_buy_prompt() {
        let snapshot = snapshot(false, true);
        let item = &snapshot.shops[0].items[0];

        assert_eq!(
            active_message(item, snapshot.shops[0].balance, "Bistro Bucks", false),
            Some("Press START to buy for 1,234 Bistro Bucks.".to_string())
        );
        assert!(!item_row_detail(item, "Bistro Bucks", false).contains("DOWNLOADED"));
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
        handle_srpg_shop_input(
            &mut state,
            &input(VirtualAction::p1_down),
            &snapshot(true, true),
        );
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

#[cfg(test)]
#[path = "srpg_shop_buffers_original.rs"]
mod buffers_original;
#[cfg(test)]
#[path = "srpg_shop_buffers_perf.rs"]
mod buffers_perf;
