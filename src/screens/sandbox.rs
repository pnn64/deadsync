use crate::act;
use crate::core::space::*;
use crate::screens::{Screen, ScreenAction};
use crate::core::input::{VirtualAction, InputEvent};
use crate::ui::actors::Actor;
// Keyboard input is handled centrally via the virtual dispatcher in app.rs

/* ---------------------------- transitions ---------------------------- */
const TRANSITION_IN_DURATION: f32 = 0.4;
const TRANSITION_OUT_DURATION: f32 = 0.4;

pub struct State {
    pub elapsed: f32,
}

pub fn init() -> State {
    State { elapsed: 0.0 }
}

// Keyboard input is handled centrally via the virtual dispatcher in app.rs

pub fn update(state: &mut State, dt: f32) {
    state.elapsed += dt;
}

pub fn in_transition() -> (Vec<Actor>, f32) {
    let actor = act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, 1.0): z(1100):
        linear(TRANSITION_IN_DURATION): alpha(0.0):
        linear(0.0): visible(false)
    );
    (vec![actor], TRANSITION_IN_DURATION)
}

pub fn out_transition() -> (Vec<Actor>, f32) {
    let actor = act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, 0.0):
        z(1200):
        linear(TRANSITION_OUT_DURATION): alpha(1.0)
    );
    (vec![actor], TRANSITION_OUT_DURATION)
}

pub fn get_actors(_state: &State) -> Vec<Actor> {
    let mut actors = Vec::with_capacity(10);

    actors.push(act!(text:
        align(0.5, 0.0): xy(screen_center_x(), 20.0):
        zoomtoheight(15.0): font("wendy"): settext("Actor System Sandbox"): horizalign(center)
    ));
    actors.push(act!(text:
        align(0.5, 0.0): xy(screen_center_x(), 60.0):
        zoomtoheight(15.0): font("miso"): settext("Press ESC or F4 to return to Menu"): horizalign(center)
    ));
    //actors.push(act!(text:
    //    align(1.0, 1.0): xy(screen_width() - 10.0, screen_height() - 10.0):
    //    zoomtoheight(15.0): font("miso"): settext(format!("Elapsed: {:.2}", state.elapsed)): horizalign(right)
    //));

    // Test 1
    //actors.push(act!(quad:
    //    diffuse(0,1,0,1)
    //));

    // Test 2
    //actors.push(act!(quad:
    //    zoomto(100,100):
    //));

    // Test 3
    //actors.push(act!(quad:
    //    zoomto(100,100):diffuse(1,0,0,1)
    //));

    // Test S1
    //actors.push(act!(sprite("logo.png"):
    //    zoom(2.0)
    //));

    // S1 - using Center
    //actors.push(act!(sprite("logo.png"):
    //    Center(): setsize(300,180)
    //));

    //C1 - using align and xy
    //actors.push(act!(sprite("logo.png"):
    //    align(0.5,0.5): xy(cx, cy): setsize(300,180)
    //));

    // C2 - using align and x and y
    // actors.push(act!(sprite("logo.png"):
    //    align(0.5,0.5): x(cx): y(cy): setsize(300,180)
    //));

    // C3 - using align and x and y
    //actors.push(act!(sprite("logo.png"):
    //    CenterX(): CenterY(): setsize(300,180)
    //));

    // TXT1 - Test text in center with miso
    //actors.push(act!(text:
    //    font("miso"): Center(): settext("Test")
    //));

    // TXT2 - Test text in center with wendy
    // actors.push(act!(text:
    //     font("wendy"): Center(): settext("Test")
    // ));

    // MW1 - maxheight(80) vs none (centered)
    // actors.push(act!(text:
    //     xy(screen_center_x(), screen_center_y()-60.0): halign(0.5): zoom(1): font("wendy"): settext("SELECT A COLOR")
    // ));

    // actors.push(act!(text:
    //     xy(screen_center_x(), screen_center_y()+60.0): halign(0.5): zoom(1): font("wendy"): settext("SELECT A COLOR"): maxwidth(220)
    // ));

    // MW2 - maxheight(80)
    actors.push(act!(text:
        xy(360,360): font("wendy"): settext("THIS STRING IS LONG"):
    ));

    actors.push(act!(text:
        xy(360,420): font("wendy"): settext("THIS STRING IS LONG"): maxwidth(240):
    ));

    actors
}

pub fn handle_input(_state: &mut State, ev: &InputEvent) -> ScreenAction {
    if ev.pressed {
        match ev.action {
            VirtualAction::p1_back => return ScreenAction::Navigate(Screen::Menu),
            _ => {}
        }
    }
    ScreenAction::None
}
